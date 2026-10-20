//! Deterministic simulated adapter covering the Phase 1 demo scenarios.
//!
//! The adapter never touches a network: it answers from fixed fixtures shaped
//! by the injected request time. Every identity it emits is synthetic and
//! every provenance is [`Provenance::Synthetic`], so demo data can never be
//! mistaken for real provider evidence. The active scenario is interior-
//! mutable so tests can switch behavior between coordinator steps.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use quotavista_core::{
    calendar::Interval,
    domain::{Availability, Provenance, QueryKind, Reason, Value},
    number::{Decimal, Money, TokenTotal},
    provider::{
        CurrentMetric, DatasetCurrent, DatasetResult, HistoryPage, HistoryPartition, ProviderError,
        ProviderPort, ProviderRequest,
    },
    storage::{Fact, FactIdentity},
};

use crate::SyntheticProvider;

/// Continuation token that ends a complete history stream.
pub const TERMINAL_CURSOR: &str = "synthetic-terminal";
/// Deterministic retry window a simulated rate limit imposes.
const RATE_LIMIT_RETRY: Duration = Duration::minutes(30);
/// Days of UTC-day history buckets a complete stream covers.
const HISTORY_DAYS: i64 = 3;

/// Selectable fixture behavior. Scenarios map to the PRD's demo states.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Scenario {
    /// Healthy values for every dataset and a complete bounded history.
    #[default]
    Nominal,
    /// Explicit zero usage, a real observation and not a missing value.
    Zero,
    /// Values the source cannot state; unknown stays unknown, never zero.
    Unknown,
    /// Usable values served from a stale upstream cache.
    StaleUpstream,
    /// One dataset denies permission while the other dataset still reports.
    PartialPermission,
    /// The token metric is absent while the money metric still reports.
    MissingTokens,
    /// Whole-call rate limit, the retry window becomes a persisted cooldown.
    RateLimited,
    /// History never attests completeness; current state stays healthy.
    IncompleteHistory,
}

/// Which datasets a provider exposes; the two demo capability fixtures.
#[derive(Clone, Copy)]
struct Capability {
    token_dataset: &'static str,
    money_dataset: &'static str,
    token_total: u128,
    spend: &'static str,
}

const KIMI: Capability = Capability {
    token_dataset: "shared-pool",
    money_dataset: "topups",
    token_total: 1_234_567_890_123,
    spend: "42.50",
};

const OPENROUTER: Capability = Capability {
    token_dataset: "key-budget",
    money_dataset: "prepaid-credits",
    token_total: 9_876_543_210_987,
    spend: "1200.00",
};

/// Network-disabled adapter for one synthetic provider.
///
/// Clones share the scenario cell so a test holder and a coordinator can
/// switch behavior mid-run.
#[derive(Clone)]
pub struct SimulatedAdapter {
    provider: SyntheticProvider,
    capability: Capability,
    scenario: Arc<Mutex<Scenario>>,
}

impl SimulatedAdapter {
    /// Adapter for a demo provider, starting from the nominal scenario.
    pub fn new(provider: SyntheticProvider) -> Self {
        let capability = if provider.id == "synthetic-kimi" {
            KIMI
        } else {
            OPENROUTER
        };
        Self {
            provider,
            capability,
            scenario: Arc::new(Mutex::new(Scenario::Nominal)),
        }
    }

    /// Synthetic identity this adapter answers for.
    pub fn provider(&self) -> &SyntheticProvider {
        &self.provider
    }

    /// Switch the active scenario mid-run; deterministic tests rely on this.
    pub fn set_scenario(&self, scenario: Scenario) {
        *self.scenario.lock().unwrap() = scenario;
    }

    /// Active scenario, for assertions.
    pub fn scenario(&self) -> Scenario {
        *self.scenario.lock().unwrap()
    }

    fn token_metric(&self, request: &ProviderRequest) -> CurrentMetric {
        let capability = self.capability;
        let scenario = self.scenario();
        let (value, availability, reason, query_kind, source_as_of) = match scenario {
            Scenario::Zero => (
                Some(Value::Tokens(TokenTotal(0))),
                Availability::Available,
                None,
                QueryKind::Unvalidated,
                Some(request.as_of),
            ),
            Scenario::Unknown => (
                None,
                Availability::Unverified,
                Some(Reason::NoData),
                QueryKind::Unvalidated,
                None,
            ),
            Scenario::StaleUpstream => (
                Some(Value::Tokens(TokenTotal(capability.token_total))),
                Availability::Available,
                None,
                QueryKind::StaleUpstreamCache,
                Some(request.as_of - Duration::hours(6)),
            ),
            Scenario::MissingTokens => (
                None,
                Availability::Missing,
                Some(Reason::NoData),
                QueryKind::Unvalidated,
                None,
            ),
            Scenario::Nominal
            | Scenario::PartialPermission
            | Scenario::RateLimited
            | Scenario::IncompleteHistory => (
                Some(Value::Tokens(TokenTotal(capability.token_total))),
                Availability::Available,
                None,
                QueryKind::Unvalidated,
                Some(request.as_of),
            ),
        };
        CurrentMetric {
            dataset: capability.token_dataset.into(),
            metric: "tokens".into(),
            product_key: None,
            value,
            availability,
            reason,
            query_kind,
            provenance: Provenance::Synthetic,
            observed_at: request.as_of,
            source_as_of,
        }
    }

    fn money_metric(&self, request: &ProviderRequest) -> CurrentMetric {
        let capability = self.capability;
        CurrentMetric {
            dataset: capability.money_dataset.into(),
            metric: "spend".into(),
            product_key: None,
            value: Some(Value::Money(Money {
                amount: capability.spend.parse::<Decimal>().unwrap(),
                currency: "USD".parse().unwrap(),
            })),
            availability: Availability::Available,
            reason: None,
            query_kind: QueryKind::Unvalidated,
            provenance: Provenance::Synthetic,
            observed_at: request.as_of,
            source_as_of: Some(request.as_of),
        }
    }

    /// Shared history partition: UTC day buckets of the token dataset.
    pub fn token_partition(&self, request: &ProviderRequest) -> HistoryPartition {
        HistoryPartition {
            dataset: self.capability.token_dataset.into(),
            product_key: String::new(),
            metric: "tokens".into(),
            interval: history_interval(request.as_of),
            grain: "P1D".into(),
            dimensions: String::new(),
            authority: Provenance::Synthetic,
        }
    }

    /// The three UTC day facts of a complete stream, starting at `first_day`.
    fn history_facts(&self, as_of: DateTime<Utc>, first_day: u32, days: u32) -> Vec<Fact> {
        let capability = self.capability;
        let start = history_interval(as_of).start;
        (first_day..first_day + days)
            .map(|day| {
                let from = start + Duration::days(i64::from(day));
                Fact {
                    identity: FactIdentity::Bucket,
                    interval: Interval::new(from, from + Duration::days(1)).unwrap(),
                    value: Value::Tokens(TokenTotal(capability.token_total + u128::from(day))),
                    source_revision: as_of.to_rfc3339(),
                }
            })
            .collect()
    }
}

/// Half-open interval of the last [`HISTORY_DAYS`] whole UTC days before `now`.
fn history_interval(now: DateTime<Utc>) -> Interval {
    let end = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let start = end - Duration::days(HISTORY_DAYS);
    Interval::new(start, end).unwrap()
}

impl ProviderPort for SimulatedAdapter {
    async fn current(
        &self,
        request: &ProviderRequest,
    ) -> Result<Vec<DatasetCurrent>, ProviderError> {
        if self.scenario() == Scenario::RateLimited {
            return Err(ProviderError::RateLimited {
                retry_after: RATE_LIMIT_RETRY,
            });
        }
        let token = self.token_metric(request);
        let money = self.money_metric(request);
        let token_result = match self.scenario() {
            Scenario::PartialPermission => DatasetResult::Failed(ProviderError::PermissionRequired),
            _ => DatasetResult::Metrics(vec![token]),
        };
        Ok(vec![
            DatasetCurrent {
                dataset: self.capability.token_dataset.into(),
                result: token_result,
            },
            DatasetCurrent {
                dataset: self.capability.money_dataset.into(),
                result: DatasetResult::Metrics(vec![money]),
            },
        ])
    }

    async fn history_page(
        &self,
        request: &ProviderRequest,
        cursor: Option<&str>,
    ) -> Result<Option<HistoryPage>, ProviderError> {
        let partition = self.token_partition(request);
        if self.scenario() == Scenario::IncompleteHistory {
            // One fact per page with an endless chain of new cursors: the
            // stream can never attest completeness, so the coordinator's page
            // bound must end it as incomplete instead of publishing.
            let next = match cursor {
                None => 1,
                Some(text) => text
                    .strip_prefix("synthetic-page-")
                    .and_then(|n| n.parse::<u32>().ok())
                    .map(|n| n + 1)
                    .unwrap_or(1),
            };
            return Ok(Some(HistoryPage {
                partition,
                facts: self.history_facts(request.as_of, (next - 1) % HISTORY_DAYS as u32, 1),
                next_cursor: Some(format!("synthetic-page-{next}")),
                complete: false,
            }));
        }
        match cursor {
            None => Ok(Some(HistoryPage {
                partition,
                facts: self.history_facts(request.as_of, 0, HISTORY_DAYS as u32),
                next_cursor: Some(TERMINAL_CURSOR.into()),
                complete: true,
            })),
            Some(TERMINAL_CURSOR) => Ok(None),
            Some(_) => Err(ProviderError::InvalidResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic_providers;

    fn request() -> ProviderRequest {
        ProviderRequest {
            connection_id: "synthetic-connection".into(),
            as_of: "2026-01-04T10:30:00Z".parse().unwrap(),
        }
    }

    fn kimi() -> SimulatedAdapter {
        SimulatedAdapter::new(synthetic_providers()[0])
    }

    fn openrouter() -> SimulatedAdapter {
        SimulatedAdapter::new(synthetic_providers()[1])
    }

    #[tokio::test]
    async fn nominal_fixtures_are_synthetic_and_available() {
        for adapter in [kimi(), openrouter()] {
            let datasets = adapter.current(&request()).await.unwrap();
            assert_eq!(datasets.len(), 2);
            for dataset in datasets {
                let DatasetResult::Metrics(metrics) = dataset.result else {
                    panic!("nominal dataset failed");
                };
                for metric in metrics {
                    assert!(metric.validate().is_ok());
                    assert_eq!(metric.provenance, Provenance::Synthetic);
                    assert_eq!(metric.query_kind, QueryKind::Unvalidated);
                }
            }
        }
    }

    #[tokio::test]
    async fn zero_unknown_stale_and_missing_scenarios_stay_explicit() {
        let adapter = kimi();
        let token_dataset = |datasets: &[DatasetCurrent]| {
            datasets
                .iter()
                .find(|d| d.dataset == "shared-pool")
                .unwrap()
                .clone()
        };

        adapter.set_scenario(Scenario::Zero);
        let DatasetResult::Metrics(zero) =
            token_dataset(&adapter.current(&request()).await.unwrap()).result
        else {
            panic!();
        };
        assert_eq!(zero[0].value, Some(Value::Tokens(TokenTotal(0))));
        assert_eq!(zero[0].availability, Availability::Available);

        adapter.set_scenario(Scenario::Unknown);
        let DatasetResult::Metrics(unknown) =
            token_dataset(&adapter.current(&request()).await.unwrap()).result
        else {
            panic!();
        };
        assert_eq!(unknown[0].value, None);
        assert_eq!(unknown[0].availability, Availability::Unverified);
        assert_eq!(unknown[0].reason, Some(Reason::NoData));

        adapter.set_scenario(Scenario::StaleUpstream);
        let DatasetResult::Metrics(stale) =
            token_dataset(&adapter.current(&request()).await.unwrap()).result
        else {
            panic!();
        };
        assert_eq!(stale[0].query_kind, QueryKind::StaleUpstreamCache);
        assert_eq!(
            stale[0].source_as_of,
            Some(request().as_of - Duration::hours(6))
        );

        adapter.set_scenario(Scenario::MissingTokens);
        let datasets = adapter.current(&request()).await.unwrap();
        let DatasetResult::Metrics(missing) = token_dataset(&datasets).result else {
            panic!();
        };
        assert_eq!(missing[0].availability, Availability::Missing);
        let money = datasets
            .iter()
            .find(|d| d.dataset == "topups")
            .unwrap()
            .clone();
        let DatasetResult::Metrics(metrics) = money.result else {
            panic!();
        };
        assert_eq!(metrics[0].availability, Availability::Available);
    }

    #[tokio::test]
    async fn partial_permission_fails_only_one_dataset() {
        let adapter = kimi();
        adapter.set_scenario(Scenario::PartialPermission);
        let datasets = adapter.current(&request()).await.unwrap();
        let token = datasets
            .iter()
            .find(|d| d.dataset == "shared-pool")
            .unwrap();
        assert_eq!(
            token.result,
            DatasetResult::Failed(ProviderError::PermissionRequired)
        );
        let money = datasets.iter().find(|d| d.dataset == "topups").unwrap();
        assert!(matches!(money.result, DatasetResult::Metrics(_)));
    }

    #[tokio::test]
    async fn rate_limited_whole_call_reports_retry_window() {
        let adapter = kimi();
        adapter.set_scenario(Scenario::RateLimited);
        let error = adapter.current(&request()).await.unwrap_err();
        assert_eq!(
            error,
            ProviderError::RateLimited {
                retry_after: RATE_LIMIT_RETRY
            }
        );
        assert_eq!(
            error.health(),
            quotavista_core::domain::QueryHealth::RateLimited
        );
        assert_eq!(error.cooldown(), Some(RATE_LIMIT_RETRY));
    }

    #[tokio::test]
    async fn complete_history_is_three_utc_day_buckets() {
        let adapter = kimi();
        let page = adapter
            .history_page(&request(), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(page.facts.len(), 3);
        assert!(page.complete);
        assert_eq!(page.next_cursor.as_deref(), Some(TERMINAL_CURSOR));
        let partition = page.partition;
        assert_eq!(partition.grain, "P1D");
        let first = page.facts[0].interval;
        assert_eq!(
            first.start,
            "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(
            first.end,
            "2026-01-02T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        // Facts stay inside the partition interval and the terminal page ends.
        assert!(page
            .facts
            .iter()
            .all(|fact| partition.interval.contains_bucket(fact.interval)));
        assert!(adapter
            .history_page(&request(), Some(TERMINAL_CURSOR))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn incomplete_history_streams_endless_partial_pages() {
        let adapter = kimi();
        adapter.set_scenario(Scenario::IncompleteHistory);
        let mut cursor = None;
        for _ in 0..5 {
            let page = adapter
                .history_page(&request(), cursor.as_deref())
                .await
                .unwrap()
                .unwrap();
            assert!(!page.complete);
            assert_eq!(page.facts.len(), 1);
            cursor = page.next_cursor;
        }
        assert_eq!(cursor.as_deref(), Some("synthetic-page-5"));
    }

    #[tokio::test]
    async fn unknown_cursor_is_invalid_response() {
        let adapter = kimi();
        assert_eq!(
            adapter.history_page(&request(), Some("not-a-cursor")).await,
            Err(ProviderError::InvalidResponse)
        );
    }
}
