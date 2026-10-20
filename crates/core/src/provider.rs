//! Provider query port and scheduling state. Phase 1 has only deterministic
//! simulated adapters behind this boundary: the port carries structured data
//! and typed errors, never URLs, credentials, or transport details, so a
//! synthetic adapter cannot issue a real provider request.

use crate::{
    calendar::Interval,
    domain::{Availability, Provenance, QueryHealth, QueryKind, Reason, Value},
    journal::{DurableStore, JournalError},
};
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;

/// Longest opaque identifier accepted in persisted scheduling state.
const MAX_ID_BYTES: usize = 128;
/// Header of the persisted cooldown file.
const COOLDOWNS_HEADER: &str = "cooldowns.v1";

/// Injectable time source. Timers must not read wall-clock time directly, so
/// scheduling behavior stays deterministic and testable.
pub trait RefreshClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// What one adapter query is about. `as_of` comes from the injected clock so
/// synthetic fixtures stay deterministic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRequest {
    pub connection_id: String,
    pub as_of: DateTime<Utc>,
}

/// Why an adapter call failed. Rate limits carry the source-provided cooldown
/// boundary; the coordinator persists it instead of retrying early.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderError {
    RateLimited { retry_after: Duration },
    PermissionRequired,
    Offline,
    Timeout,
    InvalidResponse,
}

impl ProviderError {
    /// Independent query-health classification, shared with transport states.
    pub fn health(self) -> QueryHealth {
        match self {
            Self::RateLimited { .. } => QueryHealth::RateLimited,
            Self::PermissionRequired => QueryHealth::PermissionRequired,
            Self::Offline => QueryHealth::Offline,
            Self::Timeout => QueryHealth::Timeout,
            Self::InvalidResponse => QueryHealth::InvalidResponse,
        }
    }

    /// Whether this error imposes a source cooldown the caller must persist.
    pub fn cooldown(self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after } => Some(retry_after),
            _ => None,
        }
    }
}

/// One current-state metric from a provider, before any database mapping.
/// `value.is_some()` must equal `availability == Available`, mirroring the
/// domain `Metric` invariant; unknown stays unknown and is never zero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentMetric {
    pub dataset: String,
    pub metric: String,
    /// `None` means no product breakdown.
    pub product_key: Option<String>,
    pub value: Option<Value>,
    pub availability: Availability,
    pub reason: Option<Reason>,
    pub query_kind: QueryKind,
    pub provenance: Provenance,
    pub observed_at: DateTime<Utc>,
    pub source_as_of: Option<DateTime<Utc>>,
}

impl CurrentMetric {
    pub fn validate(&self) -> Result<(), ProviderError> {
        let consistent = self.value.is_some() == (self.availability == Availability::Available)
            && self.value.as_ref().is_none_or(|v| {
                v.unit() != crate::domain::Unit::Tokens
                    || matches!(v, crate::domain::Value::Tokens(_))
            });
        if !consistent || self.dataset.is_empty() || self.metric.is_empty() {
            return Err(ProviderError::InvalidResponse);
        }
        Ok(())
    }
}

/// Outcome of one dataset's current-state query. A failed dataset retains its
/// committed values and never clears another dataset's error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DatasetResult {
    Metrics(Vec<CurrentMetric>),
    Failed(ProviderError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatasetCurrent {
    pub dataset: String,
    pub result: DatasetResult,
}

/// Partition identity of a history stream, provider side. The coordinator adds
/// the database scope identifier before staging a candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryPartition {
    pub dataset: String,
    pub product_key: String,
    pub metric: String,
    pub interval: Interval,
    pub grain: String,
    pub dimensions: String,
    pub authority: Provenance,
}

/// One bounded page of history facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryPage {
    pub partition: HistoryPartition,
    pub facts: Vec<crate::storage::Fact>,
    /// Continuation token; `None` means this is the final page.
    pub next_cursor: Option<String>,
    /// The adapter attests complete, settled, untruncated coverage of the
    /// partition interval only when `true`. An incomplete stream must never
    /// replace a published partition.
    pub complete: bool,
}

/// Generic async provider port. Implementations return structured data only;
/// retry policy belongs to the application coordinator, never the adapter.
#[allow(async_fn_in_trait)]
pub trait ProviderPort: Send + Sync {
    /// Current-state metrics per dataset, including per-dataset failures.
    async fn current(
        &self,
        request: &ProviderRequest,
    ) -> Result<Vec<DatasetCurrent>, ProviderError>;
    /// `Ok(None)` means the adapter serves no history for this connection.
    async fn history_page(
        &self,
        request: &ProviderRequest,
        cursor: Option<&str>,
    ) -> Result<Option<HistoryPage>, ProviderError>;
}

/// Persisted cooldown boundaries, one per connection, stored outside the
/// database next to the lifecycle journals so they survive restart.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Cooldowns {
    entries: BTreeMap<String, DateTime<Utc>>,
}

impl Cooldowns {
    /// Active cooldown boundary for a connection at `now`, if any.
    pub fn blocked(&self, connection_id: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.entries
            .get(connection_id)
            .filter(|until| **until > now)
            .copied()
    }

    /// Extend (never shorten) a connection's cooldown, dropping expired
    /// entries so the persisted state stays bounded.
    pub fn extend(&mut self, connection_id: &str, until: DateTime<Utc>, now: DateTime<Utc>) {
        if until <= now {
            return; // An expired boundary records nothing and never shortens.
        }
        self.entries.retain(|_, active| *active > now);
        let entry = self
            .entries
            .entry(connection_id.to_owned())
            .or_insert(until);
        if until > *entry {
            *entry = until;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn encode(&self) -> String {
        let mut text = String::from(COOLDOWNS_HEADER);
        text.push('\n');
        for (connection, until) in &self.entries {
            text.push_str(connection);
            text.push('\t');
            text.push_str(&until.to_rfc3339());
            text.push('\n');
        }
        text
    }

    /// Fail closed: a bad header, field count, identifier, or instant is a
    /// visible recovery condition, never silently ignored state.
    pub fn decode(text: &str) -> Result<Self, JournalError> {
        let mut lines = text.lines();
        if lines.next() != Some(COOLDOWNS_HEADER) {
            return Err(JournalError::Corrupt);
        }
        let mut cooldowns = Self::default();
        for line in lines {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 2
                || fields[0].is_empty()
                || fields[0].len() > MAX_ID_BYTES
                || fields[0].chars().any(char::is_control)
            {
                return Err(JournalError::Corrupt);
            }
            let until = DateTime::parse_from_rfc3339(fields[1])
                .map_err(|_| JournalError::Corrupt)?
                .with_timezone(&Utc);
            if cooldowns
                .entries
                .insert(fields[0].to_owned(), until)
                .is_some()
            {
                return Err(JournalError::Corrupt);
            }
        }
        Ok(cooldowns)
    }
}

/// Load persisted cooldowns from the durable store. A missing file is empty
/// state; anything unreadable refuses to open, like the lifecycle journals.
pub fn load_cooldowns<D: DurableStore>(durable: &D) -> Result<Cooldowns, JournalError> {
    match durable.read(crate::journal::COOLDOWNS_NAME)? {
        Some(text) => Cooldowns::decode(&text),
        None => Ok(Cooldowns::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldowns_round_trip_and_fail_closed() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let mut cooldowns = Cooldowns::default();
        assert!(cooldowns.blocked("synthetic-connection", now).is_none());
        let until = now + Duration::minutes(15);
        cooldowns.extend("synthetic-connection", until, now);
        assert_eq!(cooldowns.blocked("synthetic-connection", now), Some(until));
        assert!(cooldowns.blocked("synthetic-connection", until).is_none());
        let decoded = Cooldowns::decode(&cooldowns.encode()).unwrap();
        assert_eq!(decoded, cooldowns);
        assert_eq!(Cooldowns::decode(""), Err(JournalError::Corrupt));
        assert_eq!(
            Cooldowns::decode("cooldowns.v1\nsynthetic\tnot-an-instant"),
            Err(JournalError::Corrupt)
        );
        assert_eq!(
            Cooldowns::decode("cooldowns.v1\nsynthetic\t2026-01-01T00:00:00Z\textra"),
            Err(JournalError::Corrupt)
        );
    }

    #[test]
    fn extend_never_shortens_and_drops_expired_entries() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let mut cooldowns = Cooldowns::default();
        cooldowns.extend("a", now + Duration::minutes(15), now);
        cooldowns.extend("a", now + Duration::minutes(5), now);
        assert_eq!(
            cooldowns.blocked("a", now + Duration::minutes(10)),
            Some(now + Duration::minutes(15))
        );
        cooldowns.extend("b", now + Duration::minutes(1), now + Duration::minutes(2));
        assert!(cooldowns.blocked("b", now + Duration::minutes(2)).is_none());
        assert_eq!(cooldowns.encode().lines().count(), 2);
    }
}
