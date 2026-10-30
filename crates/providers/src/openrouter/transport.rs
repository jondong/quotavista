//! Transport boundary for the OpenRouter adapter.
//!
//! The adapter core never touches URLs, sockets, or credential storage: it
//! builds path-only requests and consumes structured responses. A transport
//! owns the base URL and applies the bearer token for one call. Tests inject
//! a fake transport, so default test runs perform no network access.
//!
//! Logging contract: nothing in this module records credentials, headers,
//! request paths with query strings, or response bodies. Error types carry
//! categories only.

use quotavista_core::secrets::{validate_secret, SecretError};
use std::fmt;
use std::time::Duration;

/// Default OpenRouter API base URL.
const DEFAULT_BASE_URL: &str = "https://openrouter.ai";
/// Per-request timeout applied by the real transport.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Hard cap on accepted response bodies. Anything larger is rejected before
/// parsing so a hostile or broken endpoint cannot exhaust memory.
const MAX_RESPONSE_BODY_BYTES: usize = 256 * 1024;

/// A redacted bearer token for one provider call.
///
/// Constructed through [`BearerToken::new`], which applies the shared secret
/// validation rules (nonempty, at most 64 KiB, no CR/LF header injection).
/// `Debug` and `Display` never expose the token text.
#[derive(Clone)]
pub struct BearerToken(String);

impl BearerToken {
    pub fn new(token: &str) -> Result<Self, SecretError> {
        validate_secret(token)?;
        Ok(Self(token.to_owned()))
    }
}

impl fmt::Debug for BearerToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BearerToken([redacted])")
    }
}

impl fmt::Display for BearerToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// Supplies the Management Key for a call. The adapter never reads
/// environment variables or secret stores itself; higher layers own
/// credential retrieval (native secret store or session memory) and hand the
/// adapter an opaque auth handle through this trait.
pub trait AuthTokenSource: Send + Sync {
    /// The token to send, or `None` when the required credential is not
    /// currently available. `None` maps to a whole-call permission failure
    /// without any network traffic and without inventing balances.
    fn management_token(&self) -> Option<BearerToken>;
}

/// One completed transport call. The body stays raw bytes; the adapter owns
/// decoding and parsing so malformed payloads surface as typed errors.
pub struct TransportResponse {
    /// HTTP status code of the response.
    pub status: u16,
    /// Raw `Retry-After` header value, when the endpoint sent one.
    pub retry_after: Option<String>,
    /// Response body, already capped to [`MAX_RESPONSE_BODY_BYTES`].
    pub body: Vec<u8>,
}

/// Why a transport call did not produce a response. Transport-level network
/// failures only; endpoint-specific outcomes (status codes, body problems)
/// travel in [`TransportResponse`] for the adapter to classify.
pub enum TransportError {
    /// Connection, DNS, or TLS failure: the provider is unreachable.
    Offline,
    /// The call exceeded its timeout budget.
    Timeout,
    /// The response body exceeded the accepted cap.
    Oversized,
}

/// Path-only GET transport for OpenRouter endpoints.
#[allow(async_fn_in_trait)]
pub trait OpenRouterTransport: Send + Sync {
    async fn get(
        &self,
        token: &BearerToken,
        path: &str,
    ) -> Result<TransportResponse, TransportError>;
}

/// Production transport over `reqwest` with rustls. Constructed with the
/// default base URL or an explicit one; always HTTPS.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    base_url: reqwest::Url,
}

impl ReqwestTransport {
    /// Transport for the public OpenRouter API.
    pub fn new() -> Result<Self, TransportInitError> {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    /// Transport for an explicit base URL (scheme must be HTTPS).
    pub fn with_base_url(base: &str) -> Result<Self, TransportInitError> {
        let base_url = reqwest::Url::parse(base)
            .ok()
            .filter(|url| url.scheme() == "https")
            .ok_or(TransportInitError::InvalidBaseUrl)?;
        let client = reqwest::Client::builder()
            // No redirects: a redirect of a credentialed GET must never be
            // followed, so the Management Key cannot leak to another origin
            // or path. The adapter treats a 3xx as an unusable response.
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { client, base_url })
    }

    fn map_error(error: reqwest::Error) -> TransportError {
        if error.is_timeout() {
            TransportError::Timeout
        } else {
            TransportError::Offline
        }
    }
}

impl fmt::Debug for ReqwestTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqwestTransport")
            .field("base_url", &self.base_url.as_str())
            .finish_non_exhaustive()
    }
}

impl OpenRouterTransport for ReqwestTransport {
    async fn get(
        &self,
        token: &BearerToken,
        path: &str,
    ) -> Result<TransportResponse, TransportError> {
        let url = self
            .base_url
            .join(path)
            .map_err(|_| TransportError::Offline)?;
        let mut response = self
            .client
            .get(url)
            .bearer_auth(&token.0)
            .timeout(REQUEST_TIMEOUT)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(Self::map_error)?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        // Stream the body in chunks and enforce the size cap *while*
        // downloading: a missing or understated Content-Length must not
        // buffer an unbounded payload in memory first.
        let mut body: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(Self::map_error)? {
            if body.len() + chunk.len() > MAX_RESPONSE_BODY_BYTES {
                return Err(TransportError::Oversized);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(TransportResponse {
            status,
            retry_after,
            body: body.to_vec(),
        })
    }
}

/// Why constructing the real transport failed.
#[derive(Debug)]
pub enum TransportInitError {
    /// The base URL was not a parseable HTTPS URL.
    InvalidBaseUrl,
    /// The HTTP client could not be built.
    Client(reqwest::Error),
}

impl From<reqwest::Error> for TransportInitError {
    fn from(error: reqwest::Error) -> Self {
        Self::Client(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_token_debug_and_display_redact_the_secret() {
        let token = BearerToken::new("sk-or-v1-synthetic-not-a-real-key").unwrap();
        let debug = format!("{token:?}");
        let display = format!("{token}");
        assert!(!debug.contains("sk-or-v1"));
        assert!(!display.contains("sk-or-v1"));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn bearer_token_rejects_invalid_secrets() {
        assert!(BearerToken::new("").is_err());
        assert!(BearerToken::new("value\r\nAuthorization: x").is_err());
        let oversized = "x".repeat(64 * 1024 + 1);
        assert!(BearerToken::new(&oversized).is_err());
    }

    #[test]
    fn base_url_must_be_https() {
        assert!(matches!(
            ReqwestTransport::with_base_url("http://openrouter.ai"),
            Err(TransportInitError::InvalidBaseUrl)
        ));
        assert!(matches!(
            ReqwestTransport::with_base_url("file:///tmp"),
            Err(TransportInitError::InvalidBaseUrl)
        ));
        assert!(ReqwestTransport::new().is_ok());
    }

    #[test]
    fn transport_debug_shows_base_url_only() {
        let transport = ReqwestTransport::new().unwrap();
        let debug = format!("{transport:?}");
        assert!(debug.contains("openrouter.ai"));
    }
}
