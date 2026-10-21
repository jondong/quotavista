//! P1-12 integration benchmark: ninety settled days of local statistics over
//! exactly 100,000 synthetic usage facts — the bounded maximum the storage
//! layer admits for one query — exercised through the same validated
//! `run_request` wire path the renderer uses.
//!
//! The fixture publishes fourteen weekly partitions of day-aligned usage
//! facts: every fact spans exactly one UTC day, the shape the aggregation
//! classifies as compatible with a UTC reporting zone (anything shorter
//! would land in `other_incompatible` and silently aggregate nothing). The
//! value of every event is a deterministic formula, so the test asserts the
//! known exact totals, per-point series values, coverage rows, and exclusion
//! sets the engine must produce — a populated aggregation can never pass by
//! returning empty-but-well-formed payloads.
//!
//! The benchmark is env-gated (`QUOTAVISTA_BENCHMARK=1`) so the default
//! `cargo test` suite stays fast, and it never touches a keychain, an
//! application directory, or a real provider: the fixture is a throwaway
//! directory with the session-only store. The JSON artifact goes to
//! `QUOTAVISTA_BENCHMARK_OUT` (default `target/p1-12-benchmark.json`); machine
//! and toolchain strings may be supplied via `QUOTAVISTA_BENCHMARK_MACHINE`
//! and `QUOTAVISTA_BENCHMARK_TOOLCHAIN`.
//!
//! The artifact records, per the phase-one plan: the seeded fact volume, the
//! query mix, five warmups plus at least thirty measured queries, per-query
//! latencies, per-mix and overall p50/p95/max against the 1-second prototype
//! target, cancellation behaviour, and the failure count. A target miss is
//! recorded, not asserted away: the benchmark test fails only on transport,
//! correctness, or consistency errors, never on latency. The 5-second
//! cancellation deadline IS enforced on the abort-to-join path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use chrono::{Datelike, Duration, NaiveDate, Utc};
use quotavista_contracts::{CommandResult, CoverageStatusDto, Envelope, ReasonDto, StatisticsDto};
use quotavista_core::calendar::Interval;
use quotavista_core::domain::{Lifecycle, Provenance, Value};
use quotavista_core::number::TokenTotal;
use quotavista_core::storage::{
    Connection, Fact, FactIdentity, Partition, Repository, Scope, WriteContext,
};
use serde_json::json;

use super::{
    build_with_provider, run_request, AppState, ChangeSink, RecordingChangeSink, SecretChoice,
};

/// The benchmark window: ninety settled UTC days, fixed so runs are
/// comparable and independent of the wall clock. 2026-07-01 is a Wednesday
/// and 2026-09-28 a Monday, so the week-grain expectations exercise both
/// clipped edges and whole interior weeks.
const START_DAY: &str = "2026-07-01";
const DAYS: i64 = 90;
/// 90 days x 1,111 events plus one extra on each of the first ten days =
/// exactly 100,000 facts, the storage bound a single query may read.
const EVENTS_PER_DAY: u64 = 1_111;
const TOTAL_FACTS: u64 = 100_000;
const CONNECTION_ID: &str = "synthetic-benchmark";
const SCOPE_ID: &str = "synthetic-scope-benchmark";
/// Prototype budget from the requirements: p95 <= 1s for the 90-day local
/// read. Recorded, not enforced, by this benchmark.
const P95_TARGET_MS: f64 = 1_000.0;
/// Architecture failure bound: a local query honours cancellation within
/// five seconds. Enforced on the abort-to-join measurement.
const CANCEL_DEADLINE_MS: u64 = 5_000;
const WARMUPS: usize = 5;
const MEASURED: usize = 30;
const CANCELLATION_ATTEMPTS: usize = 5;

fn enabled() -> bool {
    std::env::var("QUOTAVISTA_BENCHMARK").ok().as_deref() == Some("1")
}

fn canonical(command: serde_json::Value) -> serde_json::Value {
    json!({
        "contract_version": super::CONTRACT_VERSION,
        "request_id": "benchmark-request",
        "command": command,
    })
}

fn statistics_request(start: &str, end: &str, grain: &str) -> serde_json::Value {
    canonical(json!({
        "command": "query_statistics",
        "args": {
            "range": {
                "start_date": start,
                "end_date": end,
                "reporting_zone": "UTC",
                "grain": grain,
                "scope_ids": null,
            },
        },
    }))
}

fn overview_request(start: &str, end: &str) -> serde_json::Value {
    canonical(json!({
        "command": "get_overview",
        "args": {
            "range": {
                "start_date": start,
                "end_date": end,
                "reporting_zone": "UTC",
                "scope_ids": null,
            },
        },
    }))
}

fn day_date(day: i64) -> NaiveDate {
    NaiveDate::parse_from_str(START_DAY, "%Y-%m-%d").expect("fixed benchmark day parses")
        + Duration::days(day)
}

fn midnight(date: NaiveDate) -> chrono::DateTime<Utc> {
    date.and_hms_opt(0, 0, 0)
        .expect("midnight exists")
        .and_utc()
}

fn monday_of(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

/// One extra event on each of the first ten days lands the total exactly on
/// the 100,000-fact storage bound.
fn events_on(day: i64) -> u64 {
    EVENTS_PER_DAY + u64::from(day < 10)
}

/// Deterministic event value: event i of a day is worth 1 + (i % 7) tokens.
fn event_tokens(index: u64) -> u128 {
    u128::from(1 + index % 7)
}

/// The known exact token total of one seeded day.
fn day_total(day: i64) -> u128 {
    (0..events_on(day)).map(event_tokens).sum()
}

/// The known exact token total of the inclusive day range.
fn days_total(first_day: i64, last_day: i64) -> u128 {
    (first_day..=last_day).map(day_total).sum()
}

/// One benchmark connection with one scope, no refresh: the only published
/// data in the store is the seeded benchmark history. Fourteen weekly
/// partitions (full Monday-to-Monday weeks) carry the day-aligned facts, so
/// a single complete coverage row spans both day and week buckets.
async fn seed(state: &AppState) -> u64 {
    state
        .repository
        .create_connection(
            &Connection {
                id: CONNECTION_ID.to_owned(),
                provider: "synthetic-provider-benchmark".to_owned(),
                alias: "Synthetic Benchmark".to_owned(),
                generation: 0,
                lifecycle: Lifecycle::Active,
            },
            &Scope {
                id: SCOPE_ID.to_owned(),
                connection_id: CONNECTION_ID.to_owned(),
                key: "synthetic-account-benchmark".to_owned(),
                kind: "account".to_owned(),
            },
        )
        .await
        .expect("benchmark connection must be stored");

    let context = WriteContext {
        connection_id: CONNECTION_ID.to_owned(),
        generation: 0,
    };
    let mut total = 0_u64;
    let mut week_start = monday_of(day_date(0));
    let mut week_index = 0_u32;
    while week_start <= day_date(DAYS - 1) {
        let week_end = week_start + Duration::days(7);
        let run_id = format!("bench-run-w{week_index}");
        let partition = Partition {
            scope_id: SCOPE_ID.to_owned(),
            product_key: String::new(),
            dataset: "synthetic-usage".to_owned(),
            metric: "tokens".to_owned(),
            interval: Interval::new(midnight(week_start), midnight(week_end))
                .expect("one-week partition"),
            grain: "event".to_owned(),
            dimensions: String::new(),
            authority: Provenance::Synthetic,
        };
        state
            .repository
            .begin_candidate(&context, &run_id, &partition)
            .await
            .expect("begin candidate");
        // Every fact spans exactly one UTC day: the shape the aggregation
        // counts as compatible with the UTC reporting zone.
        let facts: Vec<Fact> = (0..DAYS)
            .filter(|&day| monday_of(day_date(day)) == week_start)
            .flat_map(|day| {
                let day_start = midnight(day_date(day));
                let day_end = day_start + Duration::days(1);
                (0..events_on(day)).map(move |i| Fact {
                    identity: FactIdentity::Event(format!("bench-ev-{day}-{i}")),
                    interval: Interval::new(day_start, day_end).expect("one-day fact"),
                    value: Value::Tokens(TokenTotal(event_tokens(i))),
                    source_revision: format!("bench-rev-{day}"),
                })
            })
            .collect();
        total += facts.len() as u64;
        let mut cursor: Option<String> = None;
        for (page, chunk) in facts.chunks(250).enumerate() {
            let next = format!("p{}", page + 1);
            state
                .repository
                .stage_page(&context, &run_id, cursor.as_deref(), &next, chunk)
                .await
                .expect("stage page");
            cursor = Some(next);
        }
        state
            .repository
            .complete_candidate(&context, &run_id, cursor.as_deref())
            .await
            .expect("complete candidate");
        state
            .repository
            .publish(
                &context,
                &run_id,
                midnight(week_end),
                &format!("bench-wm-{week_index}"),
            )
            .await
            .expect("publish");
        week_start = week_end;
        week_index += 1;
    }
    total
}

/// The measured query mix, with the inclusive seeded-day range each query's
/// effective interval covers. A is the worst case the interface allows:
/// ninety days at day grain over the full 100,000-fact bound.
struct Mix {
    label: &'static str,
    kind: &'static str,
    start: &'static str,
    end: &'static str,
    first_day: i64,
    last_day: i64,
    grain: &'static str,
}

const MIX: [Mix; 4] = [
    Mix {
        label: "A-statistics-90d-day",
        kind: "statistics",
        start: "2026-07-01",
        end: "2026-09-28",
        first_day: 0,
        last_day: 89,
        grain: "day",
    },
    Mix {
        label: "B-statistics-90d-week",
        kind: "statistics",
        start: "2026-07-01",
        end: "2026-09-28",
        first_day: 0,
        last_day: 89,
        grain: "week",
    },
    Mix {
        label: "C-statistics-30d-day",
        kind: "statistics",
        start: "2026-08-30",
        end: "2026-09-28",
        first_day: 60,
        last_day: 89,
        grain: "day",
    },
    // The overview summary is always day grain over the requested range.
    Mix {
        label: "D-overview-90d",
        kind: "overview",
        start: "2026-07-01",
        end: "2026-09-28",
        first_day: 0,
        last_day: 89,
        grain: "day",
    },
];

fn request_for(mix: &Mix) -> serde_json::Value {
    match mix.kind {
        "statistics" => statistics_request(mix.start, mix.end, mix.grain),
        _ => overview_request(mix.start, mix.end),
    }
}

/// The known exact series for a mix: (bucket start, token total, coverage
/// status, coverage reason) per point.
fn expected_series(
    mix: &Mix,
) -> Vec<(
    chrono::DateTime<Utc>,
    u128,
    CoverageStatusDto,
    Option<ReasonDto>,
)> {
    let first_date = day_date(mix.first_day);
    let last_date = day_date(mix.last_day);
    let mut points = Vec::new();
    match mix.grain {
        "week" => {
            let mut week = monday_of(first_date);
            while week <= last_date {
                let week_end = week + Duration::days(7);
                // Indivisible buckets are clipped at the selection edges;
                // only seeded days inside the effective interval contribute.
                let clipped = week < first_date || week_end > last_date + Duration::days(1);
                let value = (mix.first_day..=mix.last_day)
                    .filter(|&day| monday_of(day_date(day)) == week)
                    .map(day_total)
                    .sum();
                let (status, reason) = if clipped {
                    (
                        CoverageStatusDto::Partial,
                        Some(ReasonDto::BucketBoundaryMismatch),
                    )
                } else {
                    (CoverageStatusDto::Complete, None)
                };
                points.push((midnight(week.max(first_date)), value, status, reason));
                week = week_end;
            }
        }
        _ => {
            for day in mix.first_day..=mix.last_day {
                // Every seeded day is settled and spanned by one complete
                // weekly coverage row, so every day bucket is Complete.
                points.push((
                    midnight(day_date(day)),
                    day_total(day),
                    CoverageStatusDto::Complete,
                    None,
                ));
            }
        }
    }
    points
}

fn decode_statistics(envelope: Envelope) -> StatisticsDto {
    match envelope {
        Envelope::Ok { data, .. } => match *data {
            CommandResult::QueryStatistics(stats) => stats,
            other => panic!("expected QueryStatistics, got {other:?}"),
        },
        other => panic!("expected Envelope::Ok, got {other:?}"),
    }
}

/// Full known-value verification of one statistics payload: exact totals,
/// per-point values and coverage, exclusion set, and plan rows. This is the
/// proof that the measured queries aggregate the seeded 100,000 facts
/// instead of an empty compatible set.
fn assert_statistics(stats: &StatisticsDto, mix: &Mix) {
    let expected_total = days_total(mix.first_day, mix.last_day).to_string();
    assert_eq!(stats.totals.len(), 1, "{}: one totals entry", mix.label);
    assert_eq!(
        stats.totals[0].value(),
        Some(expected_total.as_str()),
        "{}: exact range total",
        mix.label
    );
    // The scope always contributes compatible facts, so nothing is excluded.
    assert!(
        stats.excluded_scopes.is_empty(),
        "{}: no excluded scopes",
        mix.label
    );
    // The fixed window is entirely in the past: nothing is provisional.
    assert!(!stats.provisional, "{}: settled window", mix.label);
    // The single seeded scope reports the same exact total as a plan row.
    assert_eq!(stats.plans.len(), 1, "{}: one plan row", mix.label);
    assert_eq!(
        stats.plans[0].metrics[0].value(),
        Some(expected_total.as_str()),
        "{}: plan row total",
        mix.label
    );

    let expected = expected_series(mix);
    assert_eq!(
        stats.series.len(),
        expected.len(),
        "{}: series length",
        mix.label
    );
    for (point, (start, value, status, reason)) in stats.series.iter().zip(&expected) {
        assert_eq!(point.zone.as_str(), "UTC", "{}: lane zone", mix.label);
        assert_eq!(
            point.interval.start.as_str(),
            start.to_rfc3339().replace("+00:00", "Z"),
            "{}: bucket start",
            mix.label
        );
        assert_eq!(
            point.metrics[0].value(),
            Some(value.to_string().as_str()),
            "{}: bucket value at {start}",
            mix.label
        );
        assert_eq!(point.coverage.len(), 1, "{}: one coverage row", mix.label);
        assert_eq!(
            point.coverage[0].status, *status,
            "{}: coverage at {start}",
            mix.label
        );
        assert_eq!(
            point.coverage[0].reason.as_ref(),
            reason.as_ref(),
            "{}: coverage reason at {start}",
            mix.label
        );
    }
}

/// The cheap per-measured-iteration check: decode outside the timed region
/// and verify the exact totals and the exclusion set. A wrong value is a
/// failure, not a slow query.
fn check_totals(mix: &Mix, envelope: Envelope) {
    let expected_total = days_total(mix.first_day, mix.last_day).to_string();
    match envelope {
        Envelope::Ok { data, .. } => {
            let stats = match *data {
                CommandResult::QueryStatistics(stats) => stats,
                CommandResult::GetOverview(overview) => {
                    assert!(
                        overview.metrics.iter().all(|metric| metric.value().is_none()),
                        "{}: no current snapshots are seeded, so no overview metric may carry a value",
                        mix.label
                    );
                    overview.summary
                }
                other => panic!("{}: unexpected result {other:?}", mix.label),
            };
            assert_eq!(
                stats.totals[0].value(),
                Some(expected_total.as_str()),
                "{}: exact range total",
                mix.label
            );
            assert!(
                stats.excluded_scopes.is_empty(),
                "{}: no excluded scopes",
                mix.label
            );
        }
        other => panic!("benchmark query {} failed: {other:?}", mix.label),
    }
}

/// Run one query through the wire path, returning (label, milliseconds).
async fn timed(state: &AppState, mix: &Mix) -> (String, f64) {
    let started = Instant::now();
    let envelope = run_request(state, Some(request_for(mix))).await;
    let ms = started.elapsed().as_secs_f64() * 1_000.0;
    check_totals(mix, envelope);
    (mix.label.to_owned(), ms)
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    let index = ((sorted.len() as f64) * q).ceil() as usize - 1;
    sorted[index.min(sorted.len() - 1)]
}

fn summary(samples: &[f64]) -> serde_json::Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({
        "count": sorted.len(),
        "p50_ms": percentile(&sorted, 0.50),
        "p95_ms": percentile(&sorted, 0.95),
        "max_ms": sorted.last().copied().unwrap_or_default(),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn statistics_query_mix_benchmark() {
    if !enabled() {
        eprintln!("benchmark skipped: set QUOTAVISTA_BENCHMARK=1 to run it");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let sink = Arc::new(RecordingChangeSink::new());
    let changes: Arc<dyn ChangeSink> = sink.clone();
    let state = build_with_provider(
        dir.path().to_path_buf(),
        changes,
        SecretChoice::SessionOnly,
        quotavista_providers::simulated::SimulatedAdapter::new(
            quotavista_providers::synthetic_providers()[0],
        ),
    )
    .await
    .expect("composition must initialise");
    let seeded = seed(&state).await;
    assert_eq!(
        seeded, TOTAL_FACTS,
        "seed lands exactly on the storage bound"
    );

    // Correctness pass, untimed: every mix must reproduce the known exact
    // totals, series values, coverage rows, and exclusion sets before a
    // single latency is trusted.
    for mix in &MIX {
        let envelope = run_request(&state, Some(request_for(mix))).await;
        match mix.kind {
            "statistics" => assert_statistics(&decode_statistics(envelope), mix),
            _ => match envelope {
                Envelope::Ok { data, .. } => match *data {
                    CommandResult::GetOverview(overview) => {
                        assert!(
                            overview.metrics.iter().all(|metric| metric.value().is_none()),
                            "no current snapshots are seeded, so no overview metric may carry a value"
                        );
                        assert_statistics(&overview.summary, mix);
                    }
                    other => panic!("expected GetOverview, got {other:?}"),
                },
                other => panic!("expected Envelope::Ok, got {other:?}"),
            },
        }
    }

    // Five warmups, unmeasured, exactly as the phase-one plan prescribes.
    for round in 0..WARMUPS {
        timed(&state, &MIX[round % MIX.len()]).await;
    }

    // Thirty measured queries across the mix: A x10, B x8, C x6, D x6.
    let plan = [("A", 10_usize), ("B", 8), ("C", 6), ("D", 6)];
    let mut samples: Vec<(String, f64)> = Vec::with_capacity(MEASURED);
    for (prefix, count) in plan {
        let mix = MIX
            .iter()
            .find(|mix| mix.label.starts_with(prefix))
            .expect("mix entry");
        for _ in 0..count {
            samples.push(timed(&state, mix).await);
        }
    }
    assert_eq!(samples.len(), MEASURED);

    // Cancellation, measured on the heaviest query. The harness runs on a
    // multi-thread runtime so the abort timer is never stuck behind the
    // synchronous aggregation: each attempt waits for the task to start,
    // lets it run 100 ms, rejects attempts that already finished, aborts,
    // and requires the join to resolve as a real cancellation inside the
    // five-second deadline. Afterwards a fresh query must reproduce the
    // pre-abort payload byte-for-byte.
    let state = Arc::new(state);
    let reference = decode_statistics(
        run_request(
            &state,
            Some(statistics_request("2026-07-01", "2026-09-28", "day")),
        )
        .await,
    );
    let reference_totals = reference.totals[0]
        .value()
        .expect("reference totals")
        .to_owned();
    let reference_series: Vec<String> = reference
        .series
        .iter()
        .map(|point| {
            point.metrics[0]
                .value()
                .expect("reference series value")
                .to_owned()
        })
        .collect();

    let mix_a = &MIX[0];
    let mut cancellations = Vec::new();
    let mut completion_flags = Vec::new();
    let mut tries = 0_u32;
    let mut valid = 0_usize;
    while valid < CANCELLATION_ATTEMPTS && tries < 3 * CANCELLATION_ATTEMPTS as u32 {
        tries += 1;
        let task_state = state.clone();
        let started = Arc::new(tokio::sync::Notify::new());
        let completed = Arc::new(AtomicBool::new(false));
        let started_in_task = started.clone();
        let completed_in_task = completed.clone();
        let mut handle = tokio::spawn(async move {
            started_in_task.notify_one();
            let envelope = run_request(&task_state, Some(request_for(mix_a))).await;
            completed_in_task.store(true, Ordering::SeqCst);
            envelope
        });
        tokio::time::timeout(StdDuration::from_secs(5), started.notified())
            .await
            .expect("query task starts");
        tokio::time::sleep(StdDuration::from_millis(100)).await;
        if completed.load(Ordering::SeqCst) {
            // The query finished before the abort could land: reject the
            // attempt instead of counting a non-cancellation.
            let _ = handle.await;
            cancellations.push(json!({
                "attempt": tries,
                "rejected": "finished-before-abort",
            }));
            continue;
        }
        let clock = Instant::now();
        handle.abort();
        let outcome =
            tokio::time::timeout(StdDuration::from_millis(CANCEL_DEADLINE_MS), &mut handle).await;
        let abort_ms = clock.elapsed().as_secs_f64() * 1_000.0;
        assert!(
            outcome.is_ok(),
            "abort-to-join honoured the {CANCEL_DEADLINE_MS} ms deadline"
        );
        let join = outcome.expect("checked above");
        let cancelled = join.as_ref().err().is_some_and(|e| e.is_cancelled());
        assert!(
            cancelled,
            "abort resolves as a task cancellation, not a panic or a silent completion"
        );
        cancellations.push(json!({
            "attempt": tries,
            "abort_to_join_ms": abort_ms,
            "cancelled_mid_flight": cancelled,
            "deadline_ms": CANCEL_DEADLINE_MS,
        }));
        completion_flags.push(completed);
        valid += 1;
    }
    assert_eq!(
        valid, CANCELLATION_ATTEMPTS,
        "enough in-flight attempts were cancelled"
    );
    // Let any uninterruptible in-flight work settle, then record whether a
    // cancelled query still ran to completion (work-completion evidence).
    tokio::time::sleep(StdDuration::from_millis(200)).await;
    let completed_after_abort = completion_flags
        .iter()
        .filter(|flag| flag.load(Ordering::SeqCst))
        .count();

    // The store survived the aborted reads: a fresh query must return the
    // exact same totals and series values as before the aborts.
    let after = decode_statistics(
        run_request(
            &state,
            Some(statistics_request("2026-07-01", "2026-09-28", "day")),
        )
        .await,
    );
    assert_eq!(
        after.totals[0].value(),
        Some(reference_totals.as_str()),
        "totals identical after aborts"
    );
    let after_series: Vec<String> = after
        .series
        .iter()
        .map(|point| point.metrics[0].value().expect("series value").to_owned())
        .collect();
    assert_eq!(
        after_series, reference_series,
        "series values identical after aborts"
    );

    let overall: Vec<f64> = samples.iter().map(|(_, ms)| *ms).collect();
    let overall_summary = summary(&overall);
    let per_mix: serde_json::Map<String, serde_json::Value> = MIX
        .iter()
        .map(|mix| {
            let latencies: Vec<f64> = samples
                .iter()
                .filter(|(l, _)| l == mix.label)
                .map(|(_, ms)| *ms)
                .collect();
            (mix.label.to_string(), summary(&latencies))
        })
        .collect();
    let p95 = overall_summary["p95_ms"].as_f64().expect("p95 recorded");
    let max_abort_ms = cancellations
        .iter()
        .filter_map(|attempt| attempt["abort_to_join_ms"].as_f64())
        .fold(0.0_f64, f64::max);

    let artifact = json!({
        "benchmark": "p1-12-local-query-mix",
        "machine": std::env::var("QUOTAVISTA_BENCHMARK_MACHINE").ok(),
        "toolchain": std::env::var("QUOTAVISTA_BENCHMARK_TOOLCHAIN").ok(),
        "platform": format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        "seed": {
            "connections": 1,
            "scopes": 1,
            "weekly_partitions": 14,
            "facts": seeded,
            "fact_shape": "each fact spans exactly one UTC day (compatible with the UTC reporting zone)",
            "window": { "start_date": "2026-07-01", "end_date": "2026-09-28", "zone": "UTC" },
        },
        "correctness": MIX
            .iter()
            .map(|mix| json!({
                "query": mix.label,
                "expected_totals": days_total(mix.first_day, mix.last_day).to_string(),
                "series_points": expected_series(mix).len(),
                "asserted": true,
            }))
            .collect::<Vec<_>>(),
        "budget": {
            "p95_target_ms": P95_TARGET_MS,
            "cancellation_deadline_ms": CANCEL_DEADLINE_MS,
        },
        "warmups": WARMUPS,
        "measured": samples.iter().map(|(label, ms)| json!({ "query": label, "ms": ms })).collect::<Vec<_>>(),
        "per_mix": per_mix,
        "overall": overall_summary,
        "within_p95_target": p95 <= P95_TARGET_MS,
        "cancellation": {
            "mechanism": "tokio JoinHandle::abort on the wire-path query task (multi-thread test runtime)",
            "valid_attempts": valid,
            "attempts": cancellations,
            "max_abort_to_join_ms": max_abort_ms,
            "deadline_ms": CANCEL_DEADLINE_MS,
            "deadline_enforced": true,
            "cancelled_queries_that_completed": completed_after_abort,
            "store_consistency": "fresh query after the aborts reproduces the exact pre-abort totals and series values",
            "query_work_cancellation_bound": {
                "status": "unverified",
                "reason": "the statistics path has no cooperative cancellation token: abort stops the task only at the next await point, and a query already inside the synchronous aggregation runs to completion before the runtime drops it. Abort-to-join latency measures consumer-facing responsiveness, not stopped work.",
                "follow_up": "Phase 2 bounded-work hardening: cooperative cancellation token through query_statistics into statistics::aggregate",
            },
        },
        "failures": 0,
    });

    let out = std::env::var("QUOTAVISTA_BENCHMARK_OUT")
        .unwrap_or_else(|_| "target/p1-12-benchmark.json".to_owned());
    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent).expect("artifact directory");
    }
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&artifact).expect("artifact serialises"),
    )
    .expect("artifact writes");
    eprintln!(
        "benchmark: {MEASURED} measured queries over {seeded} facts, overall p95 {:.1} ms (target {} ms), artifact at {out}",
        p95, P95_TARGET_MS
    );
}
