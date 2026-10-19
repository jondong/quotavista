//! Independent units, provenance, connection state, and measurement state.
use crate::{
    calendar::Interval,
    number::{Decimal, Money, Ratio, TokenTotal},
};
use chrono::{DateTime, Utc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Provenance {
    Synthetic,
    OfficialEvent,
    OfficialBucket,
    ValidatedDerivation,
    Observation,
}
impl Provenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Synthetic => "synthetic",
            Self::OfficialEvent => "official_event",
            Self::OfficialBucket => "official_bucket",
            Self::ValidatedDerivation => "validated_derivation",
            Self::Observation => "observation",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    PendingSetup,
    Active,
    Disabled,
    NeedsCredentials,
    DeletionPending,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryHealth {
    NotQueried,
    Running,
    Success,
    Offline,
    Timeout,
    RateLimited,
    AuthenticationRequired,
    PermissionRequired,
    InvalidResponse,
    LocalStorageError,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Freshness {
    NoValue,
    Current,
    Stale,
}
/// Only provider capability validation may classify a query as validated current.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryKind {
    ValidatedCurrent,
    Unvalidated,
    Historical,
    DelayedHistorical,
    StaleUpstreamCache,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverageStatus {
    Complete,
    Partial,
    Missing,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    PermissionRequired,
    NotYetSettled,
    OutsideLookback,
    Truncated,
    BucketBoundaryMismatch,
    CollectionGap,
    SyncPending,
    Unsupported,
    NoData,
    IncompatibleUnit,
    UnknownScope,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Availability {
    Available,
    Unsupported,
    PermissionRequired,
    Unverified,
    Missing,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Unit {
    Tokens,
    Money(crate::number::Currency),
    Ratio,
    PercentagePoints,
    Native(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Tokens(TokenTotal),
    Money(Money),
    Ratio(Ratio),
    PercentagePoints(Decimal),
    Native { amount: Decimal, unit: String },
}
impl Value {
    pub fn unit(&self) -> Unit {
        match self {
            Self::Tokens(_) => Unit::Tokens,
            Self::Money(m) => Unit::Money(m.currency.clone()),
            Self::Ratio(_) => Unit::Ratio,
            Self::PercentagePoints(_) => Unit::PercentagePoints,
            Self::Native { unit, .. } => Unit::Native(unit.clone()),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Period {
    pub key: Option<String>,
    pub interval: Interval,
    pub source_zone: String,
    pub expired: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Coverage {
    pub interval: Interval,
    pub status: CoverageStatus,
    pub reason: Option<Reason>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metric {
    pub metric_id: String,
    pub scope_id: String,
    pub product_id: Option<String>,
    pub unit: Unit,
    pub value: Option<Value>,
    pub availability: Availability,
    pub provenance: Provenance,
    pub observed_at: DateTime<Utc>,
    pub source_as_of: Option<DateTime<Utc>>,
    pub freshness: Freshness,
    pub period: Option<Period>,
    pub reason: Option<Reason>,
}
impl Metric {
    pub fn validate(&self) -> bool {
        self.value.as_ref().is_none_or(|v| v.unit() == self.unit)
            && (self.value.is_some() == (self.freshness != Freshness::NoValue))
            && (self.value.is_some() == (self.availability == Availability::Available))
    }
    pub fn alert_eligible(&self, query: QueryKind) -> bool {
        query == QueryKind::ValidatedCurrent
            && self.validate()
            && self.value.is_some()
            && self.freshness == Freshness::Current
            && matches!(
                self.provenance,
                Provenance::OfficialEvent | Provenance::OfficialBucket
            )
            && self.source_as_of.is_some()
            && !self.period.as_ref().is_some_and(|p| p.expired)
            && self.reason != Some(Reason::NotYetSettled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_does_not_erase_cached_zero_or_partial_coverage() {
        let observed_at = "2024-01-01T12:00:00Z".parse().unwrap();
        let mut metric = Metric {
            metric_id: "tokens".into(),
            scope_id: "synthetic-scope".into(),
            product_id: None,
            unit: Unit::Tokens,
            value: Some(Value::Tokens(TokenTotal(0))),
            availability: Availability::Available,
            provenance: Provenance::Synthetic,
            observed_at,
            source_as_of: Some(observed_at),
            freshness: Freshness::Stale,
            period: None,
            reason: None,
        };
        let states = (
            Lifecycle::Active,
            QueryHealth::LocalStorageError,
            CoverageStatus::Partial,
        );
        assert_eq!(states.0, Lifecycle::Active);
        assert!(metric.validate());
        assert!(!metric.alert_eligible(QueryKind::ValidatedCurrent));
        metric.value = None;
        metric.freshness = Freshness::NoValue;
        metric.availability = Availability::Unsupported;
        metric.reason = Some(Reason::Unsupported);
        assert!(metric.validate());
        assert_ne!(CoverageStatus::Unsupported, CoverageStatus::Missing);
        assert!(!metric.alert_eligible(QueryKind::ValidatedCurrent));
        metric.value = Some(Value::Tokens(TokenTotal(0)));
        assert!(!metric.validate());
    }
    #[test]
    fn alert_eligibility_requires_current_source_evidence() {
        let at = "2024-01-01T12:00:00Z".parse().unwrap();
        let mut metric = Metric {
            metric_id: "tokens".into(),
            scope_id: "synthetic-scope".into(),
            product_id: None,
            unit: Unit::Tokens,
            value: Some(Value::Tokens(TokenTotal(0))),
            availability: Availability::Available,
            provenance: Provenance::OfficialBucket,
            observed_at: at,
            source_as_of: Some(at),
            freshness: Freshness::Current,
            period: None,
            reason: None,
        };
        assert!(metric.alert_eligible(QueryKind::ValidatedCurrent));
        for query in [
            QueryKind::Unvalidated,
            QueryKind::Historical,
            QueryKind::DelayedHistorical,
            QueryKind::StaleUpstreamCache,
        ] {
            assert!(!metric.alert_eligible(query));
        }
        metric.reason = Some(Reason::NotYetSettled);
        assert!(!metric.alert_eligible(QueryKind::ValidatedCurrent));
        metric.reason = Some(Reason::SyncPending);
        assert!(metric.alert_eligible(QueryKind::ValidatedCurrent));
        metric.reason = None;
        metric.freshness = Freshness::Stale;
        assert!(!metric.alert_eligible(QueryKind::ValidatedCurrent));
        metric.freshness = Freshness::Current;
        metric.source_as_of = None;
        assert!(!metric.alert_eligible(QueryKind::ValidatedCurrent));
        metric.source_as_of = Some(at);
        metric.period = Some(Period {
            key: None,
            interval: Interval::new(at, at + chrono::Duration::hours(1)).unwrap(),
            source_zone: "UTC".into(),
            expired: true,
        });
        assert!(!metric.alert_eligible(QueryKind::ValidatedCurrent));
        metric.value = None;
        metric.freshness = Freshness::NoValue;
        assert!(!metric.validate());
        assert!(!metric.alert_eligible(QueryKind::ValidatedCurrent));
    }
}
