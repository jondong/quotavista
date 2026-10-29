//! Transport boundary for the Kimi adapter.
//!
//! The adapter core never touches URLs, sockets, or credential storage: it
//! builds path-only requests and consumes structured responses. A transport
//! owns the base URL and applies the session token for one call. Tests inject
//! a fake transport, so default test runs perform no network access.
//!
//! The generic wire pieces ([`BearerToken`], [`TransportResponse`],
//! [`TransportError`], [`ReqwestTransport`]) are shared with the OpenRouter
//! adapter; this module adds only the Kimi-specific auth port and transport
//! naming. The logging contract is inherited unchanged: nothing records
//! credentials, headers, request paths with query strings, or response
//! bodies, and error types carry categories only.
//!
//! Authorization model: Kimi exposes the shared membership pool through the
//! signed-in web experience, and no documented public API was known while
//! writing this skeleton. The session token therefore comes from an explicit,
//! user-authorized connection flow at runtime (native secret store or session
//! memory, owned by a higher layer). This adapter never scans or imports
//! browser cookies and never reads other tools' credentials.

use std::fmt;

pub use crate::openrouter::{
    BearerToken, ReqwestTransport, TransportError, TransportInitError, TransportResponse,
};

/// Base URL of the Kimi web application. UNVALIDATED placeholder: the exact
/// origin and membership endpoint must be confirmed against the official
/// membership page during live reconciliation (TODO live validation).
const DEFAULT_BASE_URL: &str = "https://www.kimi.com";

/// Supplies the session token for one Kimi call. The adapter never reads
/// environment variables or secret stores itself; higher layers own
/// credential retrieval (native secret store or session memory, after an
/// explicit user-authorized connection flow) and hand the adapter an opaque
/// auth handle through this trait.
pub trait SessionTokenSource: Send + Sync {
    /// The token to send, or `None` when the required credential is not
    /// currently available. `None` maps to a whole-call permission failure
    /// without any network traffic and without inventing quota values.
    fn session_token(&self) -> Option<BearerToken>;
}

/// Path-only GET transport for Kimi membership endpoints.
#[allow(async_fn_in_trait)]
pub trait KimiTransport: Send + Sync {
    async fn get(
        &self,
        token: &BearerToken,
        path: &str,
    ) -> Result<TransportResponse, TransportError>;
}

/// Production transport: the shared HTTPS `reqwest` client pinned to the
/// Kimi origin. Inherits the shared transport's guarantees: HTTPS-only base
/// URL, no redirects (a credentialed GET must never leak the session token to
/// another origin), a per-request timeout, and a hard response-size cap
/// enforced while streaming.
pub struct ReqwestKimiTransport {
    inner: ReqwestTransport,
}

impl ReqwestKimiTransport {
    /// Transport for the Kimi web origin.
    pub fn new() -> Result<Self, TransportInitError> {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    /// Transport for an explicit base URL (scheme must be HTTPS).
    pub fn with_base_url(base: &str) -> Result<Self, TransportInitError> {
        Ok(Self {
            inner: ReqwestTransport::with_base_url(base)?,
        })
    }
}

impl fmt::Debug for ReqwestKimiTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqwestKimiTransport")
            .field("inner", &self.inner)
            .finish()
    }
}

impl KimiTransport for ReqwestKimiTransport {
    async fn get(
        &self,
        token: &BearerToken,
        path: &str,
    ) -> Result<TransportResponse, TransportError> {
        crate::openrouter::OpenRouterTransport::get(&self.inner, token, path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_must_be_https() {
        assert!(matches!(
            ReqwestKimiTransport::with_base_url("http://www.kimi.com"),
            Err(TransportInitError::InvalidBaseUrl)
        ));
        assert!(matches!(
            ReqwestKimiTransport::with_base_url("file:///tmp"),
            Err(TransportInitError::InvalidBaseUrl)
        ));
        assert!(ReqwestKimiTransport::new().is_ok());
    }

    #[test]
    fn transport_debug_shows_base_url_only() {
        let transport = ReqwestKimiTransport::new().unwrap();
        let debug = format!("{transport:?}");
        assert!(debug.contains("kimi.com"));
        assert!(!debug.contains("Bearer"));
    }
}
