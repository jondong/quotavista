//! OpenRouter live adapter (skeleton).
//!
//! Query-only, read-only adapter for account-level OpenRouter data behind
//! the shared [`ProviderPort`] boundary. Phase P2-02 scope:
//!
//! * Credits (`/api/v1/credits`, Management Key) mapped to the
//!   `prepaid-credits` dataset as `total_credits`, `total_usage`, and the
//!   derived `remaining` money metrics.
//! * An explicit `Unsupported` row for the `key-budget` dataset: per-key
//!   budgets are a distinct capability from the account balance and are not
//!   queried by this adapter yet, so they are reported as unsupported
//!   instead of being conflated with account credits.
//! * Activity and Analytics endpoints are not queried yet;
//!   [`ProviderPort::history_page`] reports no history (`Ok(None)`).
//!
//! Error contract (mirrors the synthetic adapters): systemic failures fail
//! the whole call (`Err`), endpoint-specific failures fail only the
//! `prepaid-credits` dataset while other datasets keep their rows.
//! A missing Management Key fails the whole call with
//! `PermissionRequired` before any network traffic, and no balance is ever
//! invented. A 429 response surfaces as `RateLimited` with the cooldown
//! parsed from `Retry-After` (delta-seconds or HTTP-date relative to the
//! request instant), so the coordinator can persist it.
//!
//! BLOCKED on live reconciliation: no Management Key was available while
//! writing this skeleton, so wire shapes are validated against sanitized
//! fixtures only. Metrics are therefore `Unvalidated`/`Observation`, never
//! `ValidatedCurrent`. See the task's `prd.md` acceptance notes.
//!
//! Default tests run fully offline against a fake transport. The optional
//! live test at the bottom of this file runs only when a Management Key is
//! supplied locally through the documented environment variable.

mod credits;
pub mod transport;

pub use transport::{
    AuthTokenSource, BearerToken, OpenRouterTransport, ReqwestTransport, TransportError,
    TransportInitError, TransportResponse,
};

use quotavista_core::domain::{Availability, Provenance, QueryKind, Reason};
use quotavista_core::provider::{
    CurrentMetric, DatasetCurrent, DatasetResult, ProviderError, ProviderPort, ProviderRequest,
};
use std::fmt;

/// Dataset identifier for the per-key budget, shared with the synthetic
/// demo. Not queried by this adapter yet.
const KEY_BUDGET_DATASET: &str = "key-budget";

/// Fallback cooldown when a 429 response carries no usable `Retry-After`.
const DEFAULT_RETRY_AFTER_SECONDS: i64 = 60;

/// Ceiling applied to a server-provided cooldown so a hostile or broken
/// `Retry-After` cannot pin a connection for years; one day is far above any
/// documented OpenRouter rate-limit window.
const MAX_RETRY_AFTER_SECONDS: i64 = 24 * 60 * 60;

/// Environment variable that supplies an OpenRouter Management Key for the
/// optional `#[ignore]` live test. Read only inside that test, never in
/// library code: runtime credential retrieval belongs to higher layers
/// (native secret store / session memory) through [`AuthTokenSource`].
pub const LIVE_MANAGEMENT_KEY_ENV: &str = "QUOTAVISTA_OPENROUTER_MANAGEMENT_KEY";

/// Parse a `Retry-After` value: delta-seconds or an HTTP-date evaluated
/// against the request instant. Absent or unusable values fall back to
/// [`DEFAULT_RETRY_AFTER_SECONDS`]; results are clamped to
/// [`MAX_RETRY_AFTER_SECONDS`].
fn parse_retry_after(
    header: Option<&str>,
    as_of: chrono::DateTime<chrono::Utc>,
) -> chrono::Duration {
    let fallback = chrono::Duration::seconds(DEFAULT_RETRY_AFTER_SECONDS);
    let Some(value) = header.map(str::trim) else {
        return fallback;
    };
    let parsed = if value.bytes().all(|b| b.is_ascii_digit()) && !value.is_empty() {
        value.parse::<i64>().ok().map(chrono::Duration::seconds)
    } else {
        chrono::DateTime::parse_from_rfc2822(value)
            .ok()
            .map(|until| {
                until
                    .with_timezone(&chrono::Utc)
                    .signed_duration_since(as_of)
            })
    };
    let duration = parsed.unwrap_or(fallback);
    if duration > chrono::Duration::seconds(MAX_RETRY_AFTER_SECONDS) {
        return chrono::Duration::seconds(MAX_RETRY_AFTER_SECONDS);
    }
    if duration < chrono::Duration::zero() {
        return fallback;
    }
    duration
}

/// The live OpenRouter adapter. Generic over transport and auth so tests
/// inject fakes; production uses [`ReqwestTransport`] plus an
/// [`AuthTokenSource`] owned by a higher layer.
pub struct OpenRouterAdapter<T, A> {
    transport: T,
    auth: A,
}

impl<T, A> OpenRouterAdapter<T, A> {
    pub fn new(transport: T, auth: A) -> Self {
        Self { transport, auth }
    }

    /// Row stating that this adapter does not serve the key-budget dataset.
    /// Explicitly reported so account credits and per-key budgets stay
    /// distinct concepts downstream.
    fn key_budget_unsupported(request: &ProviderRequest) -> CurrentMetric {
        CurrentMetric {
            dataset: KEY_BUDGET_DATASET.to_owned(),
            metric: "tokens".to_owned(),
            product_key: None,
            value: None,
            availability: Availability::Unsupported,
            reason: Some(Reason::Unsupported),
            query_kind: QueryKind::Unvalidated,
            provenance: Provenance::Observation,
            observed_at: request.as_of,
            source_as_of: Some(request.as_of),
        }
    }

    /// Whole answer for the case where only the Credits endpoint failed with
    /// an endpoint-specific error; the key-budget row still reports
    /// unsupported so its committed data is untouched.
    fn credits_only_failure(
        error: ProviderError,
        request: &ProviderRequest,
    ) -> Vec<DatasetCurrent> {
        vec![
            DatasetCurrent {
                dataset: credits::CREDITS_DATASET.to_owned(),
                result: DatasetResult::Failed(error),
            },
            DatasetCurrent {
                dataset: KEY_BUDGET_DATASET.to_owned(),
                result: DatasetResult::Metrics(vec![Self::key_budget_unsupported(request)]),
            },
        ]
    }
}

impl<T, A> fmt::Debug for OpenRouterAdapter<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The auth source may hold a credential; never derive-print it.
        f.debug_struct("OpenRouterAdapter")
            .field("transport", &"<transport>")
            .field("auth", &"<redacted>")
            .finish()
    }
}

impl<T, A> ProviderPort for OpenRouterAdapter<T, A>
where
    T: OpenRouterTransport,
    A: AuthTokenSource,
{
    async fn current(
        &self,
        request: &ProviderRequest,
    ) -> Result<Vec<DatasetCurrent>, ProviderError> {
        // Missing credential: fail the whole call before any network traffic
        // and never invent a balance.
        let Some(token) = self.auth.management_token() else {
            return Err(ProviderError::PermissionRequired);
        };
        let response = match self.transport.get(&token, credits::CREDITS_PATH).await {
            Ok(response) => response,
            Err(TransportError::Offline) => return Err(ProviderError::Offline),
            Err(TransportError::Timeout) => return Err(ProviderError::Timeout),
            Err(TransportError::Oversized) => {
                return Ok(Self::credits_only_failure(
                    ProviderError::InvalidResponse,
                    request,
                ));
            }
        };
        match response.status {
            200..=299 => {
                let Ok(body) = String::from_utf8(response.body) else {
                    return Ok(Self::credits_only_failure(
                        ProviderError::InvalidResponse,
                        request,
                    ));
                };
                let metrics = match credits::credits_metrics(&body, request) {
                    Ok(metrics) => metrics,
                    Err(error) => return Ok(Self::credits_only_failure(error, request)),
                };
                Ok(vec![
                    DatasetCurrent {
                        dataset: credits::CREDITS_DATASET.to_owned(),
                        result: DatasetResult::Metrics(metrics),
                    },
                    DatasetCurrent {
                        dataset: KEY_BUDGET_DATASET.to_owned(),
                        result: DatasetResult::Metrics(vec![Self::key_budget_unsupported(request)]),
                    },
                ])
            }
            401 | 403 => Ok(Self::credits_only_failure(
                ProviderError::PermissionRequired,
                request,
            )),
            429 => Err(ProviderError::RateLimited {
                retry_after: parse_retry_after(response.retry_after.as_deref(), request.as_of),
            }),
            // 5xx and anything else: the provider is effectively unavailable
            // for this call; treat it as offline rather than guessing.
            _ => Err(ProviderError::Offline),
        }
    }

    async fn history_page(
        &self,
        _request: &ProviderRequest,
        _cursor: Option<&str>,
    ) -> Result<Option<quotavista_core::provider::HistoryPage>, ProviderError> {
        // Activity/Analytics history is out of P2-02 scope. Reporting no
        // history keeps committed history untouched instead of serving
        // unvalidated bucket shapes. A sanitized shape-only Activity stub
        // lives at `fixtures/activity_shape_only.json` for future work; no
        // code parses it yet.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quotavista_core::domain::Value;
    use quotavista_core::number::Money;

    /// Deterministic offline transport: one canned response, counts calls.
    /// Deterministic offline transport: one canned response, counts calls.
    struct FakeTransport {
        response: std::sync::Mutex<Option<Result<TransportResponse, TransportError>>>,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl FakeTransport {
        fn with_response(result: Result<TransportResponse, TransportError>) -> Self {
            Self {
                response: std::sync::Mutex::new(Some(result)),
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl OpenRouterTransport for FakeTransport {
        async fn get(
            &self,
            _token: &BearerToken,
            _path: &str,
        ) -> Result<TransportResponse, TransportError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.response
                .lock()
                .expect("transport lock")
                .take()
                .expect("one call expected")
        }
    }

    struct StaticToken(Option<BearerToken>);

    impl AuthTokenSource for StaticToken {
        fn management_token(&self) -> Option<BearerToken> {
            self.0.clone()
        }
    }

    fn token() -> BearerToken {
        BearerToken::new("sk-or-v1-synthetic-test-token").unwrap()
    }

    fn request() -> ProviderRequest {
        ProviderRequest {
            connection_id: "openrouter-live-1".to_owned(),
            as_of: "2026-10-29T12:00:00Z".parse().unwrap(),
        }
    }

    fn adapter(
        response: Result<TransportResponse, TransportError>,
    ) -> OpenRouterAdapter<FakeTransport, StaticToken> {
        OpenRouterAdapter::new(
            FakeTransport::with_response(response),
            StaticToken(Some(token())),
        )
    }

    fn ok_response(body: &str) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse {
            status: 200,
            retry_after: None,
            body: body.as_bytes().to_vec(),
        })
    }

    fn metric<'a>(datasets: &'a [DatasetCurrent], metric: &str) -> &'a CurrentMetric {
        let credits = datasets
            .iter()
            .find(|d| d.dataset == "prepaid-credits")
            .expect("credits dataset present");
        match &credits.result {
            DatasetResult::Metrics(metrics) => metrics
                .iter()
                .find(|m| m.metric == metric)
                .unwrap_or_else(|| panic!("metric {metric} present")),
            DatasetResult::Failed(error) => panic!("credits dataset failed: {error:?}"),
        }
    }

    #[tokio::test]
    async fn credits_success_maps_money_metrics() {
        let adapter = adapter(ok_response(include_str!("fixtures/credits_ok.json")));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(datasets.len(), 2);
        let remaining = metric(&datasets, "remaining");
        let Value::Money(Money { amount, .. }) = remaining.value.as_ref().unwrap() else {
            panic!("remaining must be money");
        };
        assert_eq!(amount.to_string(), "83.25");
        for dataset in &datasets {
            if let DatasetResult::Metrics(metrics) = &dataset.result {
                for m in metrics {
                    m.validate().unwrap();
                }
            }
        }
    }

    #[tokio::test]
    async fn key_budget_is_reported_unsupported_not_conflated() {
        let adapter = adapter(ok_response(include_str!("fixtures/credits_ok.json")));
        let datasets = adapter.current(&request()).await.unwrap();
        let key_budget = datasets
            .iter()
            .find(|d| d.dataset == "key-budget")
            .expect("key-budget dataset row");
        let DatasetResult::Metrics(metrics) = &key_budget.result else {
            panic!("key-budget row must carry metrics");
        };
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].availability, Availability::Unsupported);
        assert_eq!(metrics[0].reason, Some(Reason::Unsupported));
        assert!(metrics[0].value.is_none());
        metrics[0].validate().unwrap();
    }

    #[tokio::test]
    async fn missing_token_fails_whole_call_without_network() {
        let transport = FakeTransport::with_response(ok_response("{}"));
        let adapter = OpenRouterAdapter::new(transport, StaticToken(None));
        let result = adapter.current(&request()).await;
        assert_eq!(result, Err(ProviderError::PermissionRequired));
        assert_eq!(adapter.transport.calls(), 0);
    }

    #[tokio::test]
    async fn forbidden_maps_to_dataset_permission_failure() {
        // Only the status drives classification; the body is not parsed.
        let adapter = adapter(ok_status(
            403,
            include_str!("fixtures/credits_forbidden.json"),
        ));
        let datasets = adapter.current(&request()).await.unwrap();
        let credits = datasets
            .iter()
            .find(|d| d.dataset == "prepaid-credits")
            .unwrap();
        assert_eq!(
            credits.result,
            DatasetResult::Failed(ProviderError::PermissionRequired)
        );
    }

    fn ok_status(status: u16, body: &str) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse {
            status,
            retry_after: None,
            body: body.as_bytes().to_vec(),
        })
    }

    #[tokio::test]
    async fn unauthorized_maps_to_dataset_permission_failure() {
        let adapter = adapter(ok_status(401, "{}"));
        let datasets = adapter.current(&request()).await.unwrap();
        let credits = datasets
            .iter()
            .find(|d| d.dataset == "prepaid-credits")
            .unwrap();
        assert_eq!(
            credits.result,
            DatasetResult::Failed(ProviderError::PermissionRequired)
        );
    }

    #[tokio::test]
    async fn rate_limit_carries_cooldown_from_retry_after_seconds() {
        let response = TransportResponse {
            status: 429,
            retry_after: Some("120".to_owned()),
            body: include_str!("fixtures/credits_rate_limited.json")
                .as_bytes()
                .to_vec(),
        };
        let adapter = adapter(Ok(response));
        let result = adapter.current(&request()).await;
        assert_eq!(
            result,
            Err(ProviderError::RateLimited {
                retry_after: chrono::Duration::seconds(120),
            })
        );
    }

    #[tokio::test]
    async fn rate_limit_without_header_uses_default_cooldown() {
        let response = TransportResponse {
            status: 429,
            retry_after: None,
            body: Vec::new(),
        };
        let adapter = adapter(Ok(response));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::RateLimited {
                retry_after: chrono::Duration::seconds(DEFAULT_RETRY_AFTER_SECONDS),
            })
        );
    }

    #[tokio::test]
    async fn rate_limit_parses_http_date_against_request_instant() {
        let response = TransportResponse {
            status: 429,
            // 12:02:00 GMT is 120s after the request instant.
            retry_after: Some("Thu, 29 Oct 2026 12:02:00 GMT".to_owned()),
            body: Vec::new(),
        };
        let adapter = adapter(Ok(response));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::RateLimited {
                retry_after: chrono::Duration::seconds(120),
            })
        );
    }

    #[tokio::test]
    async fn transport_offline_fails_whole_call() {
        let adapter = adapter(Err(TransportError::Offline));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::Offline)
        );
    }

    #[tokio::test]
    async fn transport_timeout_fails_whole_call() {
        let adapter = adapter(Err(TransportError::Timeout));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::Timeout)
        );
    }

    #[tokio::test]
    async fn oversized_body_fails_credits_dataset_only() {
        let adapter = adapter(Err(TransportError::Oversized));
        let datasets = adapter.current(&request()).await.unwrap();
        let credits = datasets
            .iter()
            .find(|d| d.dataset == "prepaid-credits")
            .unwrap();
        assert_eq!(
            credits.result,
            DatasetResult::Failed(ProviderError::InvalidResponse)
        );
        assert!(datasets.iter().any(|d| d.dataset == "key-budget"));
    }

    #[tokio::test]
    async fn malformed_success_body_fails_credits_dataset_only() {
        let adapter = adapter(ok_response(include_str!("fixtures/credits_malformed.txt")));
        let datasets = adapter.current(&request()).await.unwrap();
        let credits = datasets
            .iter()
            .find(|d| d.dataset == "prepaid-credits")
            .unwrap();
        assert_eq!(
            credits.result,
            DatasetResult::Failed(ProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn non_utf8_body_fails_credits_dataset_only() {
        let response = TransportResponse {
            status: 200,
            retry_after: None,
            body: vec![0xff, 0xfe, 0x00],
        };
        let adapter = adapter(Ok(response));
        let datasets = adapter.current(&request()).await.unwrap();
        let credits = datasets
            .iter()
            .find(|d| d.dataset == "prepaid-credits")
            .unwrap();
        assert_eq!(
            credits.result,
            DatasetResult::Failed(ProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn server_error_fails_whole_call_offline() {
        let adapter = adapter(ok_status(503, "unavailable"));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::Offline)
        );
    }

    #[tokio::test]
    async fn history_is_not_served_yet() {
        let adapter = adapter(ok_response("{}"));
        assert!(adapter
            .history_page(&request(), None)
            .await
            .unwrap()
            .is_none());
        assert!(adapter
            .history_page(&request(), Some("cursor"))
            .await
            .unwrap()
            .is_none());
    }

    #[test]
    fn retry_after_parsing_clamps_and_falls_back() {
        let as_of: chrono::DateTime<chrono::Utc> = "2026-10-29T12:00:00Z".parse().unwrap();
        assert_eq!(
            parse_retry_after(Some("999999999999"), as_of),
            chrono::Duration::seconds(MAX_RETRY_AFTER_SECONDS)
        );
        assert_eq!(
            parse_retry_after(Some("not-a-date"), as_of),
            chrono::Duration::seconds(DEFAULT_RETRY_AFTER_SECONDS)
        );
        // An HTTP date in the past falls back rather than producing a
        // negative cooldown.
        assert_eq!(
            parse_retry_after(Some("Thu, 29 Oct 2026 11:00:00 GMT"), as_of),
            chrono::Duration::seconds(DEFAULT_RETRY_AFTER_SECONDS)
        );
        assert_eq!(
            parse_retry_after(None, as_of),
            chrono::Duration::seconds(DEFAULT_RETRY_AFTER_SECONDS)
        );
    }

    #[test]
    fn adapter_debug_redacts_auth() {
        let adapter = adapter(ok_response("{}"));
        let debug = format!("{adapter:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("sk-or-v1"));
    }

    /// Optional live smoke test. Never runs in CI or default `cargo test`.
    /// To run locally:
    /// `QUOTAVISTA_OPENROUTER_MANAGEMENT_KEY=<management-key> cargo test -p quotavista-providers -- --ignored`
    /// The key is a query-only Management Key; the adapter never creates,
    /// modifies, or deletes keys, and nothing about the key or account is
    /// printed.
    #[tokio::test]
    #[ignore = "requires QUOTAVISTA_OPENROUTER_MANAGEMENT_KEY (query-only Management Key)"]
    async fn quotavista_test_openrouter_live_credits_management_key() {
        let Ok(secret) = std::env::var(LIVE_MANAGEMENT_KEY_ENV) else {
            eprintln!(
                "skipping live test: {LIVE_MANAGEMENT_KEY_ENV} is not set \
                 (Management Key must be supplied locally; never committed)"
            );
            return;
        };
        let Ok(token) = BearerToken::new(&secret) else {
            eprintln!("skipping live test: {LIVE_MANAGEMENT_KEY_ENV} failed validation");
            return;
        };
        let transport = ReqwestTransport::new().expect("transport builds");
        let adapter = OpenRouterAdapter::new(transport, StaticToken(Some(token)));
        let datasets = adapter.current(&request()).await;
        match datasets {
            Err(error) => {
                // Typed whole-call outcomes (offline, timeout, rate limit) are
                // the contract working; report the category only.
                eprintln!(
                    "live call ended in typed whole-call error: {:?}",
                    error.health()
                );
            }
            Ok(datasets) => {
                for dataset in &datasets {
                    match &dataset.result {
                        DatasetResult::Failed(error) => {
                            // A key without Management scope is a meaningful
                            // observation, not a code failure.
                            eprintln!(
                                "live dataset {} failed: {:?}",
                                dataset.dataset,
                                error.health()
                            );
                        }
                        DatasetResult::Metrics(metrics) => {
                            for m in metrics {
                                m.validate().expect("live metric satisfies invariants");
                            }
                            eprintln!("live dataset {} returned metrics", dataset.dataset);
                        }
                    }
                }
            }
        }
    }
}
