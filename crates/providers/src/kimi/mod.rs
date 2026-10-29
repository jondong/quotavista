//! Kimi shared-membership live adapter (skeleton, P2-03).
//!
//! Scope: read-only retrieval of the shared membership pool — how much of the
//! pool is consumed, how much remains, and when it refreshes (requirements:
//! Kimi shared membership; A01 retrieval, A02 no substitutes, A03 reconcile).
//! The adapter implements the core [`ProviderPort`] so it composes with the
//! app state layer exactly like the OpenRouter and synthetic adapters.
//!
//! Soft dependency on P2-01 (capability fixtures): no capability reports or
//! canonical fixtures under `docs/validation/` existed when this module was
//! written, so the wire shapes in `fixtures/` are sanitized inventions.
//! Every fixture-derived value is reported with [`QueryKind::Unvalidated`]
//! until live reconciliation against the official membership page confirms
//! the response shape (TODO live validation; see
//! `.trellis/tasks/10-29-p2-03-kimi-adapter/design.md`). A01–A03 are not
//! claimed by this module.
//!
//! Scope refusal (A02): the shared membership pool is the ONLY quota this
//! adapter reads. API-wallet balances and independent Code limits are never
//! substituted for membership quota: they are reported as explicit
//! `Unsupported` rows in their own datasets (`api-wallet`, `code-limits`),
//! and a response that contains only those sections fails the `shared-pool`
//! dataset with [`ProviderError::InvalidResponse`] instead of substituting.
//!
//! Auth: the membership endpoint sits behind the signed-in web experience.
//! The adapter takes an opaque session token through
//! [`transport::SessionTokenSource`] at runtime — user-authorized, supplied
//! by a higher layer from the native secret store or session memory. It never
//! scans browser cookies, never reads other tools' credentials, and never
//! logs credentials, headers, paths with query strings, or response bodies.
//!
//! Error contract at the port boundary (mirrors the OpenRouter adapter):
//!
//! - Missing session token: the whole call fails with
//!   [`ProviderError::PermissionRequired`] before any network traffic —
//!   provider state must never be silently reported with no data behind it.
//! - Offline or timeout: the whole call fails with
//!   [`ProviderError::Offline`]/[`ProviderError::Timeout`]; the app-state
//!   refresh path only replaces values on success, so a failed call
//!   preserves the last successful values.
//! - HTTP 401/403: the `shared-pool` dataset row reports
//!   `PermissionRequired`; the `Unsupported` rows stay populated so the app
//!   still reflects the correct state of what can never be read.
//! - HTTP 429: the whole call fails with [`ProviderError::RateLimited`] with
//!   the cooldown parsed from `Retry-After` (delta-seconds or HTTP-date,
//!   default 60 s, clamped to 24 h) via the shared hardened parser.
//! - Oversized, non-UTF-8, malformed, or schema-mismatching bodies fail the
//!   `shared-pool` dataset row with [`ProviderError::InvalidResponse`];
//!   nothing is clamped or guessed.

mod membership;
pub mod transport;

pub use transport::{
    BearerToken, KimiTransport, ReqwestKimiTransport, SessionTokenSource, TransportError,
    TransportInitError, TransportResponse,
};

use quotavista_core::domain::{Availability, Provenance, QueryKind, Reason};
use quotavista_core::provider::{
    CurrentMetric, DatasetCurrent, DatasetResult, HistoryPage, ProviderError, ProviderPort,
    ProviderRequest,
};

use membership::{membership_metrics, MEMBERSHIP_PATH, SHARED_POOL_DATASET};

/// Dataset identifier for the API wallet, reported only as `Unsupported`:
/// a wallet balance is a different capability from membership quota and is
/// never substituted for it.
const API_WALLET_DATASET: &str = "api-wallet";
/// Dataset identifier for independent Code limits, reported only as
/// `Unsupported`: same reasoning as the API wallet.
const CODE_LIMITS_DATASET: &str = "code-limits";

/// Environment variable that supplies a Kimi session token for the optional
/// `#[ignore]` live test. Read only inside that test, never in library code:
/// runtime credential retrieval belongs to higher layers (native secret
/// store / session memory) through [`SessionTokenSource`].
pub const LIVE_SESSION_TOKEN_ENV: &str = "QUOTAVISTA_KIMI_SESSION_TOKEN";

/// Read-only live adapter for the Kimi shared membership pool. Generic over
/// auth and transport so tests inject fakes; production uses
/// [`ReqwestKimiTransport`] plus a [`SessionTokenSource`] owned by a higher
/// layer.
pub struct KimiMembershipAdapter<T, A> {
    transport: T,
    auth: A,
}

impl<T, A> KimiMembershipAdapter<T, A> {
    pub fn new(transport: T, auth: A) -> Self {
        Self { transport, auth }
    }
}

impl<T, A> std::fmt::Debug for KimiMembershipAdapter<T, A> {
    /// The auth source may hold a credential; never derive-print it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KimiMembershipAdapter")
            .field("transport", &"<transport>")
            .field("auth", &"<redacted>")
            .finish()
    }
}

impl<T: KimiTransport, A: SessionTokenSource> KimiMembershipAdapter<T, A> {
    /// One `Unsupported` row: this adapter never substitutes a different
    /// quota surface for the shared membership pool.
    fn unsupported_row(dataset: &str, metric: &str, request: &ProviderRequest) -> DatasetCurrent {
        DatasetCurrent {
            dataset: dataset.to_owned(),
            result: DatasetResult::Metrics(vec![CurrentMetric {
                dataset: dataset.to_owned(),
                metric: metric.to_owned(),
                product_key: None,
                value: None,
                availability: Availability::Unsupported,
                reason: Some(Reason::Unsupported),
                query_kind: QueryKind::Unvalidated,
                provenance: Provenance::Observation,
                observed_at: request.as_of,
                source_as_of: None,
            }]),
        }
    }

    /// Whole answer for the case where the membership endpoint failed with an
    /// endpoint-specific error; the unsupported rows still report refusal so
    /// their committed data is untouched.
    fn pool_only_failure(error: ProviderError, request: &ProviderRequest) -> Vec<DatasetCurrent> {
        vec![
            DatasetCurrent {
                dataset: SHARED_POOL_DATASET.to_owned(),
                result: DatasetResult::Failed(error),
            },
            Self::unsupported_row(API_WALLET_DATASET, "balance", request),
            Self::unsupported_row(CODE_LIMITS_DATASET, "tokens", request),
        ]
    }
}

impl<T: KimiTransport, A: SessionTokenSource> ProviderPort for KimiMembershipAdapter<T, A> {
    async fn current(
        &self,
        request: &ProviderRequest,
    ) -> Result<Vec<DatasetCurrent>, ProviderError> {
        // Missing credential: fail the whole call before any network traffic
        // and never invent membership quota.
        let Some(token) = self.auth.session_token() else {
            return Err(ProviderError::PermissionRequired);
        };
        let response = match self.transport.get(&token, MEMBERSHIP_PATH).await {
            Ok(response) => response,
            Err(TransportError::Offline) => return Err(ProviderError::Offline),
            Err(TransportError::Timeout) => return Err(ProviderError::Timeout),
            Err(TransportError::Oversized) => {
                return Ok(Self::pool_only_failure(
                    ProviderError::InvalidResponse,
                    request,
                ));
            }
        };
        match response.status {
            200..=299 => {
                let Ok(body) = String::from_utf8(response.body) else {
                    return Ok(Self::pool_only_failure(
                        ProviderError::InvalidResponse,
                        request,
                    ));
                };
                match membership_metrics(&body, request) {
                    Ok(metrics) => Ok(vec![
                        DatasetCurrent {
                            dataset: SHARED_POOL_DATASET.to_owned(),
                            result: DatasetResult::Metrics(metrics),
                        },
                        Self::unsupported_row(API_WALLET_DATASET, "balance", request),
                        Self::unsupported_row(CODE_LIMITS_DATASET, "tokens", request),
                    ]),
                    Err(error) => Ok(Self::pool_only_failure(error, request)),
                }
            }
            401 | 403 => Ok(Self::pool_only_failure(
                ProviderError::PermissionRequired,
                request,
            )),
            429 => Err(ProviderError::RateLimited {
                retry_after: crate::openrouter::parse_retry_after(
                    response.retry_after.as_deref(),
                    request.as_of,
                ),
            }),
            // Any other received-but-unusable status (400/404/422/5xx …): the
            // endpoint answered and the response cannot be trusted for
            // accounting; that is an unusable response, not an offline
            // network condition.
            _ => Err(ProviderError::InvalidResponse),
        }
    }

    async fn history_page(
        &self,
        _request: &ProviderRequest,
        _cursor: Option<&str>,
    ) -> Result<Option<HistoryPage>, ProviderError> {
        // Membership history is out of P2-03 scope. Reporting no history
        // keeps committed history untouched instead of serving unvalidated
        // bucket shapes.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use quotavista_core::domain::Value;
    use quotavista_core::number::Ratio;
    use std::sync::Mutex;

    /// Deterministic offline transport: one canned responder, records paths.
    struct FakeTransport {
        calls: Mutex<Vec<String>>,
        respond: Box<dyn Fn() -> Result<TransportResponse, TransportError> + Send + Sync>,
    }

    impl FakeTransport {
        fn with_response(result: Result<TransportResponse, TransportError>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                respond: Box::new(move || {
                    result.as_ref().map_or_else(
                        |error| {
                            Err(match error {
                                TransportError::Offline => TransportError::Offline,
                                TransportError::Timeout => TransportError::Timeout,
                                TransportError::Oversized => TransportError::Oversized,
                            })
                        },
                        |response| {
                            Ok(TransportResponse {
                                status: response.status,
                                retry_after: response.retry_after.clone(),
                                body: response.body.clone(),
                            })
                        },
                    )
                }),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl KimiTransport for FakeTransport {
        async fn get(
            &self,
            _token: &BearerToken,
            path: &str,
        ) -> Result<TransportResponse, TransportError> {
            self.calls.lock().unwrap().push(path.to_owned());
            (self.respond)()
        }
    }

    struct StaticToken(Option<BearerToken>);

    impl SessionTokenSource for StaticToken {
        fn session_token(&self) -> Option<BearerToken> {
            self.0.clone()
        }
    }

    fn token() -> BearerToken {
        BearerToken::new("kimi-synthetic-test-token").unwrap()
    }

    fn request() -> ProviderRequest {
        ProviderRequest {
            connection_id: "kimi-live-1".to_owned(),
            as_of: "2026-10-29T12:00:00Z".parse().unwrap(),
        }
    }

    fn adapter(
        result: Result<TransportResponse, TransportError>,
    ) -> KimiMembershipAdapter<FakeTransport, StaticToken> {
        KimiMembershipAdapter::new(
            FakeTransport::with_response(result),
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

    fn ok_status(status: u16, body: &str) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse {
            status,
            retry_after: None,
            body: body.as_bytes().to_vec(),
        })
    }

    fn dataset<'a>(datasets: &'a [DatasetCurrent], name: &str) -> &'a DatasetCurrent {
        datasets
            .iter()
            .find(|d| d.dataset == name)
            .unwrap_or_else(|| panic!("dataset {name} present"))
    }

    fn assert_unsupported_row(datasets: &[DatasetCurrent], name: &str) {
        let DatasetResult::Metrics(metrics) = &dataset(datasets, name).result else {
            panic!("unsupported row must carry metrics: {name}");
        };
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].availability, Availability::Unsupported);
        assert_eq!(metrics[0].reason, Some(Reason::Unsupported));
        assert!(metrics[0].value.is_none());
        metrics[0].validate().unwrap();
    }

    #[tokio::test]
    async fn success_maps_shared_pool_and_keeps_unsupported_rows() {
        let adapter = adapter(ok_response(include_str!("fixtures/membership_ok.json")));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(datasets.len(), 3);
        let pool = dataset(&datasets, SHARED_POOL_DATASET);
        let DatasetResult::Metrics(metrics) = &pool.result else {
            panic!("shared pool must carry metrics");
        };
        let remaining = metrics
            .iter()
            .find(|m| m.metric == "quota_remaining")
            .expect("quota_remaining present");
        assert_eq!(
            remaining.value,
            Some(Value::Ratio("0.7".parse::<Ratio>().unwrap()))
        );
        let consumed = metrics
            .iter()
            .find(|m| m.metric == "quota_consumed")
            .expect("quota_consumed present");
        assert_eq!(
            consumed.value,
            Some(Value::PercentagePoints("30".parse().unwrap()))
        );
        for m in metrics {
            m.validate().unwrap();
        }
        assert_unsupported_row(&datasets, API_WALLET_DATASET);
        assert_unsupported_row(&datasets, CODE_LIMITS_DATASET);
    }

    #[tokio::test]
    async fn wallet_only_payload_fails_pool_without_substituting() {
        // The envelope answers API-wallet and Code-limit sections but no
        // shared pool: membership quota stays unknown, nothing substitutes.
        let adapter = adapter(ok_response(include_str!(
            "fixtures/membership_wallet_only.json"
        )));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(datasets.len(), 3);
        assert_eq!(
            dataset(&datasets, SHARED_POOL_DATASET).result,
            DatasetResult::Failed(ProviderError::InvalidResponse)
        );
        assert_unsupported_row(&datasets, API_WALLET_DATASET);
        assert_unsupported_row(&datasets, CODE_LIMITS_DATASET);
    }

    #[tokio::test]
    async fn missing_token_fails_whole_call_without_network() {
        let transport = FakeTransport::with_response(ok_response("{}"));
        let adapter = KimiMembershipAdapter::new(transport, StaticToken(None));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::PermissionRequired)
        );
        assert!(adapter.transport.calls().is_empty());
    }

    #[tokio::test]
    async fn forbidden_maps_to_pool_permission_failure() {
        // Only the status drives classification; the body is not parsed.
        let adapter = adapter(ok_status(
            403,
            include_str!("fixtures/membership_forbidden.json"),
        ));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(
            dataset(&datasets, SHARED_POOL_DATASET).result,
            DatasetResult::Failed(ProviderError::PermissionRequired)
        );
        assert_unsupported_row(&datasets, API_WALLET_DATASET);
        assert_unsupported_row(&datasets, CODE_LIMITS_DATASET);
    }

    #[tokio::test]
    async fn unauthorized_maps_to_pool_permission_failure() {
        let adapter = adapter(ok_status(401, "{}"));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(
            dataset(&datasets, SHARED_POOL_DATASET).result,
            DatasetResult::Failed(ProviderError::PermissionRequired)
        );
    }

    #[tokio::test]
    async fn rate_limit_carries_cooldown_from_retry_after_seconds() {
        let response = TransportResponse {
            status: 429,
            retry_after: Some("120".to_owned()),
            body: include_str!("fixtures/membership_rate_limited.json")
                .as_bytes()
                .to_vec(),
        };
        let adapter = adapter(Ok(response));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::RateLimited {
                retry_after: Duration::seconds(120),
            })
        );
    }

    #[tokio::test]
    async fn rate_limit_without_header_uses_default_cooldown() {
        let adapter = adapter(ok_status(429, "{}"));
        assert_eq!(
            adapter.current(&request()).await,
            Err(ProviderError::RateLimited {
                retry_after: Duration::seconds(60),
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
    async fn oversized_body_fails_pool_dataset_only() {
        let adapter = adapter(Err(TransportError::Oversized));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(
            dataset(&datasets, SHARED_POOL_DATASET).result,
            DatasetResult::Failed(ProviderError::InvalidResponse)
        );
        assert_unsupported_row(&datasets, API_WALLET_DATASET);
    }

    #[tokio::test]
    async fn malformed_success_body_fails_pool_dataset_only() {
        let adapter = adapter(ok_response(include_str!(
            "fixtures/membership_malformed.txt"
        )));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(
            dataset(&datasets, SHARED_POOL_DATASET).result,
            DatasetResult::Failed(ProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn non_utf8_body_fails_pool_dataset_only() {
        let response = TransportResponse {
            status: 200,
            retry_after: None,
            body: vec![0xff, 0xfe, 0x00],
        };
        let adapter = adapter(Ok(response));
        let datasets = adapter.current(&request()).await.unwrap();
        assert_eq!(
            dataset(&datasets, SHARED_POOL_DATASET).result,
            DatasetResult::Failed(ProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn server_error_fails_whole_call_invalid_response() {
        // A 503 was *received*: unusable-response semantics, not offline.
        let adapter_503 = adapter(ok_status(503, "unavailable"));
        assert_eq!(
            adapter_503.current(&request()).await,
            Err(ProviderError::InvalidResponse)
        );
        // Same for a client error that is not an auth failure.
        let adapter_404 = adapter(ok_status(404, "not found"));
        assert_eq!(
            adapter_404.current(&request()).await,
            Err(ProviderError::InvalidResponse)
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
    fn adapter_debug_redacts_auth() {
        let adapter = adapter(ok_response("{}"));
        let debug = format!("{adapter:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("kimi-synthetic-test-token"));
    }

    #[test]
    fn bearer_token_debug_and_display_are_redacted() {
        let token = BearerToken::new("kimi-synthetic-secret").unwrap();
        let rendered = format!("{token:?} {token}");
        assert!(!rendered.contains("kimi-synthetic-secret"));
        assert!(rendered.contains("[redacted]"));
    }

    /// Optional live smoke test. Never runs in CI or default `cargo test`.
    /// To run locally:
    /// `QUOTAVISTA_KIMI_SESSION_TOKEN=<session-token> cargo test -p quotavista-providers -- --ignored`
    /// The token must be supplied locally by the user after an explicit,
    /// user-authorized connection flow; it is never committed, logged, or
    /// printed, and nothing about the account is recorded. The test asserts
    /// only the provenance contract, never specific quota values.
    #[tokio::test]
    #[ignore = "requires QUOTAVISTA_KIMI_SESSION_TOKEN (user-authorized session token)"]
    async fn quotavista_test_kimi_live_membership_session_token() {
        let Ok(secret) = std::env::var(LIVE_SESSION_TOKEN_ENV) else {
            eprintln!(
                "skipping live test: {LIVE_SESSION_TOKEN_ENV} is not set \
                 (session token must be supplied locally; never committed)"
            );
            return;
        };
        let Ok(token) = BearerToken::new(&secret) else {
            eprintln!("skipping live test: {LIVE_SESSION_TOKEN_ENV} failed validation");
            return;
        };
        let transport = ReqwestKimiTransport::new().expect("transport builds");
        let adapter = KimiMembershipAdapter::new(transport, StaticToken(Some(token)));
        let datasets = adapter.current(&request()).await;
        match datasets {
            Err(error) => {
                // Typed whole-call outcomes (offline, timeout, rate limit)
                // are the contract working; report the category only.
                eprintln!("live call ended in typed whole-call error: {error:?}");
            }
            Ok(datasets) => {
                for dataset in &datasets {
                    match &dataset.result {
                        DatasetResult::Failed(error) => {
                            // A session without membership-page access is a
                            // meaningful observation, not a code failure.
                            eprintln!("live dataset {} failed: {error:?}", dataset.dataset);
                        }
                        DatasetResult::Metrics(metrics) => {
                            for m in metrics {
                                m.validate().expect("live metric satisfies invariants");
                                assert_eq!(m.query_kind, QueryKind::Unvalidated);
                            }
                            eprintln!("live dataset {} returned metrics", dataset.dataset);
                        }
                    }
                }
            }
        }
    }
}
