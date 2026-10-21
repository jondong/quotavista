//! Deterministic aggregation of stored facts into overview and period
//! statistics. Pure domain logic: no I/O, clocks, or transport types.
//!
//! Rules implemented here:
//! - totals sum only complete compatible source buckets inside the effective
//!   interval; current snapshots (account/key representations) never enter
//!   range totals;
//! - a source bucket is compatible when it is exactly one calendar day of the
//!   reporting zone; UTC-only buckets stay in a separate UTC lane and their
//!   scopes are excluded from the common total with a reason;
//! - missing token data yields a null total, never a zero; a real zero
//!   requires a recorded zero fact;
//! - the current unsettled day is clipped at the captured as-of instant and
//!   reported as partial with `not_yet_settled`.

use crate::{
    calendar::{self, CalendarRange, Interval},
    domain::{
        Availability, Coverage, CoverageStatus, Freshness, Grain, Metric, Provenance, QueryKind,
        Reason, Value,
    },
    number::{NumericError, TokenTotal},
    storage::{CurrentValue, StoredPartition},
};
use chrono::{DateTime, Days, Duration, NaiveDate, Utc};
use chrono_tz::Tz;

/// Scope identifier carried by cross-scope totals.
pub const TOTAL_SCOPE: &str = "all-scopes";
/// Metric identifier of recorded-token totals and per-bucket values.
pub const RECORDED_TOKENS: &str = "recorded_tokens";
/// Synthetic quota-remaining metric (shared pool allowance fraction).
pub const QUOTA_REMAINING: &str = "quota_remaining";
/// Synthetic quota-consumed metric (percentage points of the allowance).
pub const QUOTA_CONSUMED: &str = "quota_consumed";
/// Synthetic recorded-token current reading metric.
pub const TOKENS: &str = "tokens";
/// Synthetic money consumption metric.
pub const SPEND: &str = "spend";

/// Current metrics reported per scope, in canonical order.
const CURRENT_METRICS: [&str; 4] = [TOKENS, QUOTA_REMAINING, QUOTA_CONSUMED, SPEND];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatisticsError {
    /// A day or week boundary does not exist in the reporting zone.
    Boundary,
    /// A checked total overflowed.
    Overflow,
}

impl From<NumericError> for StatisticsError {
    fn from(value: NumericError) -> Self {
        match value {
            NumericError::Overflow => Self::Overflow,
            _ => Self::Boundary,
        }
    }
}

/// One scope's stored view for a statistics query.
#[derive(Clone, Debug)]
pub struct ScopeInput {
    pub scope_id: String,
    /// Synthetic plan identifier: the provider's token dataset name.
    pub token_dataset: String,
    /// Whether the serving provider can report shared-quota metrics.
    pub quota_supported: bool,
    pub snapshots: Vec<CurrentValue>,
    pub partitions: Vec<StoredPartition>,
}

/// A domain metric plus the query kind the read path derived for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricEntry {
    pub metric: Metric,
    pub query_kind: QueryKind,
}

/// One series point; `zone` names the calendar the interval follows, so a
/// UTC-only lane is never disguised as reporting-zone days.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesPoint {
    pub interval: Interval,
    pub zone: Tz,
    pub metric: MetricEntry,
    pub coverage: Vec<Coverage>,
}

/// A scope excluded from a total, with the reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExcludedScope {
    pub scope_id: String,
    pub metric_id: String,
    pub reason: Reason,
}

/// Per-plan (scope, native unit) row of a statistics result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanEntry {
    pub scope_id: String,
    pub product_id: String,
    pub metrics: Vec<MetricEntry>,
}

#[derive(Clone, Debug)]
pub struct Statistics {
    pub requested: Interval,
    pub effective: Vec<Interval>,
    pub zone: Tz,
    pub grain: Grain,
    pub as_of: DateTime<Utc>,
    pub provisional: bool,
    pub coverage: Vec<Coverage>,
    pub excluded: Vec<ExcludedScope>,
    pub totals: Vec<MetricEntry>,
    pub series: Vec<SeriesPoint>,
    pub plans: Vec<PlanEntry>,
}

/// True when the bucket is exactly one calendar day of `zone`.
fn is_zone_day(bucket: Interval, zone: Tz) -> bool {
    let date = bucket.start.with_timezone(&zone).date_naive();
    let Some(next) = date.checked_add_days(Days::new(1)) else {
        return false;
    };
    match (
        calendar::midnight(date, zone),
        calendar::midnight(next, zone),
    ) {
        (Ok(start), Ok(end)) => start == bucket.start && end == bucket.end,
        // A day whose midnight does not exist cannot be a zone day.
        _ => false,
    }
}

/// True when the bucket is exactly one UTC day (fixed 24 hours from midnight).
fn is_utc_day(bucket: Interval) -> bool {
    bucket.start.time() == chrono::NaiveTime::MIN && bucket.end == bucket.start + Duration::days(1)
}

/// Grain buckets intersecting the effective interval, as (point, natural)
/// pairs: the point is the natural bucket clipped to the effective interval.
fn grain_buckets(
    effective: Interval,
    grain: Grain,
    zone: Tz,
) -> Result<Vec<(Interval, Interval)>, StatisticsError> {
    let first = effective.start.with_timezone(&zone).date_naive();
    let last = (effective.end - Duration::nanoseconds(1))
        .with_timezone(&zone)
        .date_naive();
    let mut buckets = Vec::new();
    match grain {
        Grain::Day => {
            let mut date = first;
            while date <= last {
                buckets.push(day_bucket(date, effective, zone)?);
                date = date
                    .checked_add_days(Days::new(1))
                    .ok_or(StatisticsError::Boundary)?;
            }
        }
        Grain::Week => {
            let mut week = calendar::monday(first).map_err(|_| StatisticsError::Boundary)?;
            while calendar::midnight(week, zone).map_err(|_| StatisticsError::Boundary)?
                < effective.end
            {
                let next = week
                    .checked_add_days(Days::new(7))
                    .ok_or(StatisticsError::Boundary)?;
                let natural = Interval {
                    start: calendar::midnight(week, zone).map_err(|_| StatisticsError::Boundary)?,
                    end: calendar::midnight(next, zone).map_err(|_| StatisticsError::Boundary)?,
                };
                if natural.end > effective.start {
                    buckets.push(clip(natural, effective)?);
                }
                week = next;
            }
        }
    }
    Ok(buckets)
}

fn day_bucket(
    date: NaiveDate,
    effective: Interval,
    zone: Tz,
) -> Result<(Interval, Interval), StatisticsError> {
    let next = date
        .checked_add_days(Days::new(1))
        .ok_or(StatisticsError::Boundary)?;
    let natural = Interval {
        start: calendar::midnight(date, zone).map_err(|_| StatisticsError::Boundary)?,
        end: calendar::midnight(next, zone).map_err(|_| StatisticsError::Boundary)?,
    };
    clip(natural, effective)
}

fn clip(natural: Interval, effective: Interval) -> Result<(Interval, Interval), StatisticsError> {
    let start = natural.start.max(effective.start);
    let end = natural.end.min(effective.end);
    if start >= end {
        return Err(StatisticsError::Boundary);
    }
    Ok((Interval { start, end }, natural))
}

/// Freshness of a persisted current value: a value is current when its source
/// timestamp is at least as recent as the sync that stored it.
fn snapshot_freshness(value: &CurrentValue) -> Freshness {
    match value.snapshot.source_as_of {
        Some(source_as_of) if source_as_of >= value.successful_sync_at => Freshness::Current,
        _ => Freshness::Stale,
    }
}

/// A snapshot as a metric entry with derived freshness and query kind.
fn snapshot_metric(value: &CurrentValue) -> MetricEntry {
    let snapshot = &value.snapshot;
    let freshness = snapshot_freshness(value);
    MetricEntry {
        metric: Metric {
            metric_id: snapshot.metric.clone(),
            scope_id: snapshot.scope_id.clone(),
            product_id: snapshot.product_id.clone(),
            unit: snapshot.value.unit(),
            value: Some(snapshot.value.clone()),
            availability: Availability::Available,
            provenance: snapshot.provenance,
            observed_at: snapshot.observed_at,
            source_as_of: snapshot.source_as_of,
            freshness,
            period: None,
            reason: None,
        },
        query_kind: match freshness {
            Freshness::Stale => QueryKind::StaleUpstreamCache,
            _ => QueryKind::Unvalidated,
        },
    }
}

/// A metric entry for a state that has no value (missing or unsupported).
fn state_metric(
    metric_id: &str,
    scope_id: &str,
    availability: Availability,
    reason: Reason,
    as_of: DateTime<Utc>,
) -> MetricEntry {
    let unit = match metric_id {
        TOKENS | RECORDED_TOKENS => crate::domain::Unit::Tokens,
        QUOTA_REMAINING => crate::domain::Unit::Ratio,
        QUOTA_CONSUMED => crate::domain::Unit::PercentagePoints,
        // A missing spend marker still carries the money unit (the fixture
        // currency is USD); only genuinely unknown ids fall back to tokens.
        SPEND => crate::domain::Unit::Money("USD".parse().expect("literal currency code is valid")),
        _ => crate::domain::Unit::Tokens,
    };
    MetricEntry {
        metric: Metric {
            metric_id: metric_id.into(),
            scope_id: scope_id.into(),
            product_id: None,
            unit,
            value: None,
            availability,
            provenance: Provenance::Synthetic,
            observed_at: as_of,
            source_as_of: None,
            freshness: Freshness::NoValue,
            period: None,
            reason: Some(reason),
        },
        query_kind: QueryKind::Unvalidated,
    }
}

/// Current metrics of one scope in canonical order, including explicit
/// missing and unsupported markers for expected metrics without a snapshot.
pub fn current_metrics(scope: &ScopeInput, as_of: DateTime<Utc>) -> Vec<MetricEntry> {
    let mut metrics = Vec::new();
    let mut snapshots: Vec<&CurrentValue> = scope.snapshots.iter().collect();
    snapshots.sort_by(|a, b| a.snapshot.metric.cmp(&b.snapshot.metric));
    let mut emitted = Vec::new();
    for id in CURRENT_METRICS {
        if let Some(value) = snapshots.iter().find(|v| v.snapshot.metric == id) {
            metrics.push(snapshot_metric(value));
        } else if id == QUOTA_REMAINING || id == QUOTA_CONSUMED {
            if scope.quota_supported {
                metrics.push(state_metric(
                    id,
                    &scope.scope_id,
                    Availability::Missing,
                    Reason::NoData,
                    as_of,
                ));
            } else {
                metrics.push(state_metric(
                    id,
                    &scope.scope_id,
                    Availability::Unsupported,
                    Reason::Unsupported,
                    as_of,
                ));
            }
        } else {
            metrics.push(state_metric(
                id,
                &scope.scope_id,
                Availability::Missing,
                Reason::NoData,
                as_of,
            ));
        }
        emitted.push(id);
    }
    for value in snapshots {
        if !emitted.contains(&value.snapshot.metric.as_str()) {
            metrics.push(snapshot_metric(value));
        }
    }
    metrics
}

struct ScopeFacts {
    compatible: Vec<(Interval, TokenTotal)>,
    utc: Vec<(Interval, TokenTotal)>,
    /// Token facts that are neither reporting-zone days nor UTC days.
    other_incompatible: usize,
    complete_coverage: Vec<Interval>,
}

fn scope_facts(scope: &ScopeInput, zone: Tz) -> ScopeFacts {
    let mut facts = ScopeFacts {
        compatible: Vec::new(),
        utc: Vec::new(),
        other_incompatible: 0,
        complete_coverage: Vec::new(),
    };
    for stored in &scope.partitions {
        for row in &stored.coverage {
            // A coverage row only evidences coverage in the lane whose calendar
            // it follows; a UTC row never covers reporting-zone-local days.
            if row.status == CoverageStatus::Complete && row.zone == zone.name() {
                facts.complete_coverage.push(row.interval);
            }
        }
        for fact in &stored.facts {
            // Only token facts aggregate; money and native units never mix
            // into a token total.
            let Value::Tokens(tokens) = fact.value else {
                continue;
            };
            if is_zone_day(fact.interval, zone) {
                facts.compatible.push((fact.interval, tokens));
            } else if is_utc_day(fact.interval) {
                facts.utc.push((fact.interval, tokens));
            } else {
                facts.other_incompatible += 1;
            }
        }
    }
    facts.complete_coverage.sort_by_key(|i| i.start);
    facts
}

fn missing_reason(scopes: &[&ScopeFacts], point: Interval) -> Reason {
    let mut lookback = false;
    for facts in scopes {
        let intersects = facts
            .utc
            .iter()
            .any(|(iv, _)| iv.start < point.end && iv.end > point.start);
        if intersects {
            // The scope has facts here, but their boundaries do not match the
            // reporting zone, so they were excluded from this lane.
            return Reason::BucketBoundaryMismatch;
        }
        match facts.complete_coverage.first() {
            None => return Reason::CollectionGap,
            Some(earliest) if point.start < earliest.start => lookback = true,
            _ => {}
        }
    }
    if lookback {
        Reason::OutsideLookback
    } else {
        Reason::CollectionGap
    }
}

/// Aggregate the stored view into period statistics for the validated range.
pub fn aggregate(
    range: &CalendarRange,
    grain: Grain,
    scopes: &[ScopeInput],
) -> Result<Statistics, StatisticsError> {
    let zone = range.reporting_zone;
    let as_of = range.as_of;
    let facts: Vec<ScopeFacts> = scopes.iter().map(|s| scope_facts(s, zone)).collect();

    let mut series = Vec::new();
    let mut lane_coverage = Vec::new();
    if let Some(effective) = range.effective {
        for (point, natural) in grain_buckets(effective, grain, zone)? {
            let unsettled = natural.end > as_of;
            let clipped = point.start != natural.start || point.end != natural.end;
            let mut sum = TokenTotal::default();
            let mut any = false;
            for scope in &facts {
                for (iv, tokens) in &scope.compatible {
                    if point.contains_bucket(*iv) {
                        sum = sum.checked_add(*tokens)?;
                        any = true;
                    }
                }
            }
            let covered = scopes
                .iter()
                .zip(&facts)
                .filter(|(_, f)| {
                    f.complete_coverage
                        .iter()
                        .any(|iv| iv.contains_bucket(point))
                })
                .count();
            let (status, reason) = if unsettled {
                (
                    if any {
                        CoverageStatus::Partial
                    } else {
                        CoverageStatus::Missing
                    },
                    Some(Reason::NotYetSettled),
                )
            } else if clipped {
                // A grain bucket clipped by the selection keeps the edge partial.
                (
                    CoverageStatus::Partial,
                    Some(Reason::BucketBoundaryMismatch),
                )
            } else if !scopes.is_empty() && covered == scopes.len() {
                (CoverageStatus::Complete, None)
            } else {
                let uncovered: Vec<&ScopeFacts> = scopes
                    .iter()
                    .zip(&facts)
                    .filter(|(_, f)| {
                        !f.complete_coverage
                            .iter()
                            .any(|iv| iv.contains_bucket(point))
                    })
                    .map(|(_, f)| f)
                    .collect();
                let reason = missing_reason(&uncovered, point);
                (
                    if covered == 0 {
                        CoverageStatus::Missing
                    } else {
                        CoverageStatus::Partial
                    },
                    Some(reason),
                )
            };
            let coverage = vec![Coverage {
                interval: point,
                status,
                reason,
            }];
            lane_coverage.push(coverage[0].clone());
            series.push(SeriesPoint {
                interval: point,
                zone,
                metric: MetricEntry {
                    metric: Metric {
                        metric_id: RECORDED_TOKENS.into(),
                        scope_id: TOTAL_SCOPE.into(),
                        product_id: None,
                        unit: crate::domain::Unit::Tokens,
                        value: any.then_some(Value::Tokens(sum)),
                        availability: if any {
                            Availability::Available
                        } else {
                            Availability::Missing
                        },
                        provenance: Provenance::ValidatedDerivation,
                        observed_at: as_of,
                        source_as_of: None,
                        freshness: if any {
                            Freshness::Current
                        } else {
                            Freshness::NoValue
                        },
                        period: None,
                        reason: if any { None } else { reason },
                    },
                    query_kind: QueryKind::Historical,
                },
                coverage,
            });
        }
    }

    // UTC-only lane: one point per distinct UTC day bucket, actual boundaries.
    let mut utc_days: Vec<(Interval, TokenTotal)> = Vec::new();
    for scope in &facts {
        for (iv, tokens) in &scope.utc {
            // Only complete source buckets fully inside the selected interval
            // belong to the lane; out-of-range or edge-crossing buckets must
            // not be drawn as part of the selection.
            let in_range = range
                .effective
                .is_some_and(|effective| effective.contains_bucket(*iv));
            if !in_range {
                continue;
            }
            match utc_days.iter_mut().find(|(day, _)| *day == *iv) {
                Some((_, total)) => *total = total.checked_add(*tokens)?,
                None => utc_days.push((*iv, *tokens)),
            }
        }
    }
    utc_days.sort_by_key(|(iv, _)| iv.start);
    for (interval, total) in utc_days {
        series.push(SeriesPoint {
            interval,
            zone: Tz::UTC,
            metric: MetricEntry {
                metric: Metric {
                    metric_id: RECORDED_TOKENS.into(),
                    scope_id: TOTAL_SCOPE.into(),
                    product_id: None,
                    unit: crate::domain::Unit::Tokens,
                    value: Some(Value::Tokens(total)),
                    availability: Availability::Available,
                    provenance: Provenance::ValidatedDerivation,
                    observed_at: as_of,
                    source_as_of: None,
                    freshness: Freshness::Current,
                    period: None,
                    reason: None,
                },
                query_kind: QueryKind::Historical,
            },
            coverage: vec![Coverage {
                interval,
                status: CoverageStatus::Complete,
                reason: None,
            }],
        });
    }

    // Exclusions and the recorded-token total.
    let mut excluded = Vec::new();
    let mut total = TokenTotal::default();
    let mut any_compatible = false;
    let mut any_incompatible = false;
    for (scope, facts) in scopes.iter().zip(&facts) {
        // A scope is excluded only when it contributes nothing compatible:
        // compatible facts stay in the total, and any UTC-only days of a
        // contributing scope are visible in the UTC lane instead.
        let excluded_reason = if !facts.compatible.is_empty() {
            None
        } else if !facts.utc.is_empty() || facts.other_incompatible > 0 {
            any_incompatible = true;
            Some(Reason::BucketBoundaryMismatch)
        } else {
            Some(Reason::NoData)
        };
        if let Some(reason) = excluded_reason {
            excluded.push(ExcludedScope {
                scope_id: scope.scope_id.clone(),
                metric_id: RECORDED_TOKENS.into(),
                reason,
            });
        }
        if let Some(effective) = range.effective {
            for (iv, tokens) in &facts.compatible {
                if effective.contains_bucket(*iv) {
                    total = total.checked_add(*tokens)?;
                    any_compatible = true;
                }
            }
        }
    }
    let totals = vec![MetricEntry {
        metric: Metric {
            metric_id: RECORDED_TOKENS.into(),
            scope_id: TOTAL_SCOPE.into(),
            product_id: None,
            unit: crate::domain::Unit::Tokens,
            value: any_compatible.then_some(Value::Tokens(total)),
            availability: if any_compatible {
                Availability::Available
            } else {
                Availability::Missing
            },
            provenance: Provenance::ValidatedDerivation,
            observed_at: as_of,
            source_as_of: None,
            freshness: if any_compatible {
                Freshness::Current
            } else {
                Freshness::NoValue
            },
            period: None,
            reason: if any_compatible {
                None
            } else if any_incompatible {
                Some(Reason::BucketBoundaryMismatch)
            } else {
                Some(Reason::NoData)
            },
        },
        query_kind: QueryKind::Historical,
    }];

    let mut plans = Vec::new();
    for (scope, facts) in scopes.iter().zip(&facts) {
        let mut metrics = Vec::new();
        let mut sum = TokenTotal::default();
        let mut any = false;
        if let Some(effective) = range.effective {
            for (iv, tokens) in &facts.compatible {
                if effective.contains_bucket(*iv) {
                    sum = sum.checked_add(*tokens)?;
                    any = true;
                }
            }
        }
        let reason = if any {
            None
        } else if !facts.utc.is_empty() || facts.other_incompatible > 0 {
            Some(Reason::BucketBoundaryMismatch)
        } else {
            Some(Reason::NoData)
        };
        metrics.push(MetricEntry {
            metric: Metric {
                metric_id: RECORDED_TOKENS.into(),
                scope_id: scope.scope_id.clone(),
                product_id: None,
                unit: crate::domain::Unit::Tokens,
                value: any.then_some(Value::Tokens(sum)),
                availability: if any {
                    Availability::Available
                } else {
                    Availability::Missing
                },
                provenance: Provenance::ValidatedDerivation,
                observed_at: as_of,
                source_as_of: None,
                freshness: if any {
                    Freshness::Current
                } else {
                    Freshness::NoValue
                },
                period: None,
                reason,
            },
            query_kind: QueryKind::Historical,
        });
        metrics.extend(current_metrics(scope, as_of));
        plans.push(PlanEntry {
            scope_id: scope.scope_id.clone(),
            product_id: scope.token_dataset.clone(),
            metrics,
        });
    }

    Ok(Statistics {
        requested: range.requested,
        effective: range.effective.into_iter().collect(),
        zone,
        grain,
        as_of,
        provisional: range.provisional,
        coverage: lane_coverage,
        excluded,
        totals,
        series,
        plans,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        calendar::FakeClock,
        number::TokenCount,
        storage::{CoverageRow, Fact, FactIdentity, Partition},
    };

    fn instant(text: &str) -> DateTime<Utc> {
        crate::calendar::parse_instant(text).unwrap()
    }

    fn utc_partition(scope: &str, day: NaiveDate, tokens: u64) -> StoredPartition {
        let start =
            DateTime::<Utc>::from_naive_utc_and_offset(day.and_hms_opt(0, 0, 0).unwrap(), Utc);
        StoredPartition {
            partition: Partition {
                scope_id: scope.into(),
                product_key: String::new(),
                dataset: "shared-pool".into(),
                metric: TOKENS.into(),
                interval: Interval {
                    start,
                    end: start + Duration::days(3),
                },
                grain: "P1D".into(),
                dimensions: String::new(),
                authority: Provenance::Synthetic,
            },
            facts: (0..3)
                .map(|offset| Fact {
                    identity: FactIdentity::Bucket,
                    interval: Interval {
                        start: start + Duration::days(offset),
                        end: start + Duration::days(offset + 1),
                    },
                    value: Value::Tokens(TokenTotal(u128::from(tokens + offset as u64))),
                    source_revision: "synthetic".into(),
                })
                .collect(),
            coverage: vec![CoverageRow {
                interval: Interval {
                    start,
                    end: start + Duration::days(3),
                },
                status: CoverageStatus::Complete,
                reason: None,
                zone: "UTC".into(),
            }],
            data_revision: 1,
            successful_sync_at: start,
            watermark: "synthetic".into(),
        }
    }

    fn scope(scope_id: &str) -> ScopeInput {
        ScopeInput {
            scope_id: scope_id.into(),
            token_dataset: "shared-pool".into(),
            quota_supported: true,
            snapshots: Vec::new(),
            partitions: Vec::new(),
        }
    }

    fn range(start: &str, end: &str, zone: &str, now: &str) -> CalendarRange {
        calendar::calendar_range(start, end, zone, &FakeClock(instant(now))).unwrap()
    }

    fn total_value(statistics: &Statistics) -> Option<TokenTotal> {
        match &statistics.totals[0].metric.value {
            Some(Value::Tokens(total)) => Some(*total),
            _ => None,
        }
    }

    #[test]
    fn utc_zone_day_grain_sums_exact_known_totals() {
        // 2024-03-01 is a Friday; the partition covers Mar 1..4 with 3 facts.
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.partitions.push(utc_partition(
            "synthetic-scope-kimi",
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            1_000,
        ));
        let statistics = aggregate(
            &range("2024-03-01", "2024-03-04", "UTC", "2024-03-04T12:00:00Z"),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        // Exact total: 1000 + 1001 + 1002; today (Mar 4) is clipped and unsettled.
        assert_eq!(total_value(&statistics), Some(TokenTotal(3_003)));
        assert_eq!(statistics.series.len(), 4);
        assert!(statistics.provisional);
        let today = &statistics.series[3];
        assert_eq!(today.interval.end, instant("2024-03-04T12:00:00Z"));
        assert_eq!(today.coverage[0].status, CoverageStatus::Missing);
        assert_eq!(today.coverage[0].reason, Some(Reason::NotYetSettled));
        assert_eq!(today.metric.metric.value, None);
        let settled = &statistics.series[0];
        assert_eq!(settled.coverage[0].status, CoverageStatus::Complete);
        assert_eq!(
            settled.metric.metric.value,
            Some(Value::Tokens(TokenTotal(1_000)))
        );
        assert!(statistics.excluded.is_empty());
    }

    #[test]
    fn snapshots_never_enter_range_totals() {
        // The scope also carries a cumulative current reading (the account/key
        // representation); the range total stays the exact bucket sum.
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.partitions.push(utc_partition(
            "synthetic-scope-kimi",
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            1_000,
        ));
        kimi.snapshots.push(CurrentValue {
            snapshot: crate::storage::Snapshot {
                scope_id: "synthetic-scope-kimi".into(),
                product_id: None,
                metric: TOKENS.into(),
                value: Value::Tokens(TokenTotal(9_999_999_999)),
                provenance: Provenance::Synthetic,
                observed_at: instant("2024-03-04T00:00:00Z"),
                source_as_of: Some(instant("2024-03-04T00:00:00Z")),
            },
            successful_sync_at: instant("2024-03-04T00:00:00Z"),
            data_revision: 1,
        });
        let statistics = aggregate(
            &range("2024-03-01", "2024-03-04", "UTC", "2024-03-04T12:00:00Z"),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        assert_eq!(total_value(&statistics), Some(TokenTotal(3_003)));
    }

    #[test]
    fn non_utc_zone_keeps_utc_buckets_in_a_separate_lane() {
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.partitions.push(utc_partition(
            "synthetic-scope-kimi",
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            1_000,
        ));
        let statistics = aggregate(
            &range(
                "2024-03-01",
                "2024-03-03",
                "America/New_York",
                "2024-03-04T12:00:00Z",
            ),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        // No compatible facts: the total is null, not zero.
        assert_eq!(total_value(&statistics), None);
        assert_eq!(
            statistics.totals[0].metric.reason,
            Some(Reason::BucketBoundaryMismatch)
        );
        assert_eq!(
            statistics.excluded,
            vec![ExcludedScope {
                scope_id: "synthetic-scope-kimi".into(),
                metric_id: RECORDED_TOKENS.into(),
                reason: Reason::BucketBoundaryMismatch,
            }]
        );
        // Reporting-lane points stay missing; UTC points carry the values.
        let utc_points: Vec<&SeriesPoint> = statistics
            .series
            .iter()
            .filter(|p| p.zone == Tz::UTC)
            .collect();
        // Only UTC buckets fully inside the selection appear: Mar 2 and Mar 3
        // are in, while the Mar 1 bucket crosses the left edge (05:00Z) and
        // stays out of the lane.
        assert_eq!(utc_points.len(), 2);
        assert_eq!(
            utc_points[0].interval,
            Interval {
                start: instant("2024-03-02T00:00:00Z"),
                end: instant("2024-03-03T00:00:00Z"),
            }
        );
        assert!(utc_points
            .iter()
            .all(|p| p.coverage[0].status == CoverageStatus::Complete));
        let local_points: Vec<&SeriesPoint> = statistics
            .series
            .iter()
            .filter(|p| p.zone != Tz::UTC)
            .collect();
        assert_eq!(local_points.len(), 3);
        assert!(local_points.iter().all(|p| p.metric.metric.value.is_none()));
        assert!(local_points
            .iter()
            .all(|p| p.coverage[0].reason == Some(Reason::BucketBoundaryMismatch)));
    }

    #[test]
    fn utc_lane_only_shows_buckets_fully_inside_the_selection() {
        // Regression: a single New York day overlaps the seeded 3-day UTC
        // partition, but no UTC bucket is fully inside it, so the lane must
        // be empty rather than listing out-of-range buckets as complete.
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.partitions.push(utc_partition(
            "synthetic-scope-kimi",
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            1_000,
        ));
        let statistics = aggregate(
            &range(
                "2024-03-02",
                "2024-03-02",
                "America/New_York",
                "2024-03-04T12:00:00Z",
            ),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        assert!(statistics.series.iter().all(|p| p.zone != Tz::UTC));
        assert_eq!(total_value(&statistics), None);
        assert_eq!(
            statistics.excluded,
            vec![ExcludedScope {
                scope_id: "synthetic-scope-kimi".into(),
                metric_id: RECORDED_TOKENS.into(),
                reason: Reason::BucketBoundaryMismatch,
            }]
        );
    }

    #[test]
    fn mixed_scope_stays_in_the_total_and_is_not_listed_as_excluded() {
        // One compatible reporting-zone day plus UTC-only days: the
        // compatible fact stays in the total and the UTC days are visible in
        // the lane, so the scope must not carry an exclusion entry.
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.partitions.push(utc_partition(
            "synthetic-scope-kimi",
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            1_000,
        ));
        let ny_day = Interval {
            start: instant("2024-03-02T05:00:00Z"),
            end: instant("2024-03-03T05:00:00Z"),
        };
        kimi.partitions.push(StoredPartition {
            partition: Partition {
                scope_id: "synthetic-scope-kimi".into(),
                product_key: String::new(),
                dataset: "shared-pool".into(),
                metric: TOKENS.into(),
                interval: ny_day,
                grain: "P1D".into(),
                dimensions: String::new(),
                authority: Provenance::Synthetic,
            },
            facts: vec![Fact {
                identity: FactIdentity::Bucket,
                interval: ny_day,
                value: Value::Tokens(TokenTotal(42)),
                source_revision: "synthetic".into(),
            }],
            coverage: vec![CoverageRow {
                interval: ny_day,
                status: CoverageStatus::Complete,
                reason: None,
                zone: "America/New_York".into(),
            }],
            data_revision: 1,
            successful_sync_at: ny_day.end,
            watermark: "synthetic".into(),
        });
        let statistics = aggregate(
            &range(
                "2024-03-01",
                "2024-03-03",
                "America/New_York",
                "2024-03-04T12:00:00Z",
            ),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        assert_eq!(total_value(&statistics), Some(TokenTotal(42)));
        assert!(statistics.excluded.is_empty());
        let utc_points: Vec<&SeriesPoint> = statistics
            .series
            .iter()
            .filter(|p| p.zone == Tz::UTC)
            .collect();
        // The Mar 2 and Mar 3 UTC buckets are fully inside; Mar 1 crosses.
        assert_eq!(utc_points.len(), 2);
    }

    #[test]
    fn utc_zone_week_grain_keeps_clipped_edge_partial() {
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.partitions.push(utc_partition(
            "synthetic-scope-kimi",
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            1_000,
        ));
        // Range starts Wednesday 2024-02-28; the week starting Monday 02-26
        // crosses the range start and stays partial.
        let statistics = aggregate(
            &range("2024-02-28", "2024-03-03", "UTC", "2024-03-04T12:00:00Z"),
            Grain::Week,
            &[kimi],
        )
        .unwrap();
        assert_eq!(statistics.series.len(), 1);
        let week = &statistics.series[0];
        assert_eq!(week.interval.start, instant("2024-02-28T00:00:00Z"));
        assert_eq!(week.interval.end, instant("2024-03-04T00:00:00Z"));
        assert_eq!(week.coverage[0].status, CoverageStatus::Partial);
        assert_eq!(
            week.coverage[0].reason,
            Some(Reason::BucketBoundaryMismatch)
        );
        // Only the complete buckets inside the clipped week are summed.
        assert_eq!(
            week.metric.metric.value,
            Some(Value::Tokens(TokenTotal(3_003)))
        );
        assert_eq!(total_value(&statistics), Some(TokenTotal(3_003)));
    }

    #[test]
    fn missing_scope_is_excluded_with_no_data_and_never_zero() {
        let kimi = scope("synthetic-scope-kimi");
        let statistics = aggregate(
            &range("2024-03-01", "2024-03-03", "UTC", "2024-03-04T12:00:00Z"),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        assert_eq!(total_value(&statistics), None);
        assert_eq!(statistics.totals[0].metric.reason, Some(Reason::NoData));
        assert_eq!(
            statistics.excluded,
            vec![ExcludedScope {
                scope_id: "synthetic-scope-kimi".into(),
                metric_id: RECORDED_TOKENS.into(),
                reason: Reason::NoData,
            }]
        );
        assert!(statistics
            .series
            .iter()
            .all(|p| p.metric.metric.value.is_none()));
    }

    #[test]
    fn recorded_zero_is_a_real_value() {
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.partitions.push(utc_partition(
            "synthetic-scope-kimi",
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            0,
        ));
        // Facts are 0, 1, 2: the total distinguishes a recorded zero from null.
        let statistics = aggregate(
            &range("2024-03-01", "2024-03-01", "UTC", "2024-03-04T12:00:00Z"),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        assert_eq!(total_value(&statistics), Some(TokenTotal(0)));
        assert_eq!(
            statistics.totals[0].metric.availability,
            Availability::Available
        );
    }

    #[test]
    fn quota_and_spend_stay_visible_when_tokens_are_missing() {
        let mut kimi = scope("synthetic-scope-kimi");
        let snapshot = |metric: &str, value: Value| CurrentValue {
            snapshot: crate::storage::Snapshot {
                scope_id: "synthetic-scope-kimi".into(),
                product_id: None,
                metric: metric.into(),
                value,
                provenance: Provenance::Synthetic,
                observed_at: instant("2024-03-04T00:00:00Z"),
                source_as_of: Some(instant("2024-03-04T00:00:00Z")),
            },
            successful_sync_at: instant("2024-03-04T00:00:00Z"),
            data_revision: 1,
        };
        kimi.snapshots.push(snapshot(
            QUOTA_REMAINING,
            Value::Ratio("0.70".parse().unwrap()),
        ));
        kimi.snapshots.push(snapshot(
            QUOTA_CONSUMED,
            Value::PercentagePoints("30".parse().unwrap()),
        ));
        let statistics = aggregate(
            &range("2024-03-01", "2024-03-03", "UTC", "2024-03-04T12:00:00Z"),
            Grain::Day,
            std::slice::from_ref(&kimi),
        )
        .unwrap();
        let plan = &statistics.plans[0];
        assert_eq!(plan.product_id, "shared-pool");
        let quota = plan
            .metrics
            .iter()
            .find(|m| m.metric.metric_id == QUOTA_REMAINING)
            .unwrap();
        assert_eq!(
            quota.metric.value,
            Some(Value::Ratio("0.70".parse().unwrap()))
        );
        assert_eq!(quota.metric.freshness, Freshness::Current);
        let tokens = &plan.metrics[0];
        assert_eq!(tokens.metric.metric_id, RECORDED_TOKENS);
        assert_eq!(tokens.metric.value, None);
        assert_eq!(tokens.metric.reason, Some(Reason::NoData));
        // Native quota consumption remains visible even with tokens missing.
        let consumed = plan
            .metrics
            .iter()
            .find(|m| m.metric.metric_id == QUOTA_CONSUMED)
            .unwrap();
        assert_eq!(
            consumed.metric.value,
            Some(Value::PercentagePoints("30".parse().unwrap()))
        );
    }

    #[test]
    fn unsupported_quota_is_explicit_for_providers_without_it() {
        let mut openrouter = scope("synthetic-scope-openrouter");
        openrouter.quota_supported = false;
        let metrics = current_metrics(&openrouter, instant("2024-03-04T12:00:00Z"));
        let quota = metrics
            .iter()
            .find(|m| m.metric.metric_id == QUOTA_REMAINING)
            .unwrap();
        assert_eq!(quota.metric.availability, Availability::Unsupported);
        assert_eq!(quota.metric.reason, Some(Reason::Unsupported));
        assert_eq!(quota.metric.value, None);
    }

    #[test]
    fn stale_snapshot_is_stale_upstream_cache() {
        let mut kimi = scope("synthetic-scope-kimi");
        kimi.snapshots.push(CurrentValue {
            snapshot: crate::storage::Snapshot {
                scope_id: "synthetic-scope-kimi".into(),
                product_id: None,
                metric: TOKENS.into(),
                value: Value::Tokens(TokenTotal::from(TokenCount(7))),
                provenance: Provenance::Synthetic,
                observed_at: instant("2024-03-04T00:00:00Z"),
                source_as_of: Some(instant("2024-03-03T18:00:00Z")),
            },
            successful_sync_at: instant("2024-03-04T00:00:00Z"),
            data_revision: 1,
        });
        let metrics = current_metrics(&kimi, instant("2024-03-04T12:00:00Z"));
        let tokens = metrics
            .iter()
            .find(|m| m.metric.metric_id == TOKENS)
            .unwrap();
        assert_eq!(tokens.metric.freshness, Freshness::Stale);
        assert_eq!(tokens.query_kind, QueryKind::StaleUpstreamCache);
    }

    #[test]
    fn empty_effective_interval_reports_missing_not_zero() {
        // The whole range lies after as-of: no effective interval remains.
        let kimi = scope("synthetic-scope-kimi");
        let statistics = aggregate(
            &range("2024-03-04", "2024-03-04", "UTC", "2024-03-04T00:00:00Z"),
            Grain::Day,
            &[kimi],
        )
        .unwrap();
        assert!(statistics.effective.is_empty());
        assert!(statistics.series.is_empty());
        assert_eq!(total_value(&statistics), None);
    }
}
