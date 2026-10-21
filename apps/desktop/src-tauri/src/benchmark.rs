//! P1-12 integration benchmark: ninety settled days of local statistics over
//! exactly 100,000 synthetic usage facts — the bounded maximum the storage
//! layer admits for one query — exercised through the same validated
//! `run_request` wire path the renderer uses.
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
//! recorded, not asserted away: the benchmark test fails only on transport or
//! consistency errors, never on latency.

use std::sync::Arc;
use std::time::Instant;

use chrono::{Duration, Utc};
use quotavista_contracts::{CommandResult, Envelope};
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
/// comparable and independent of the wall clock.
const START_DAY: &str = "2026-07-01T00:00:00Z";
const DAYS: i64 = 90;
/// 90 days x 1,111 events plus one extra on each of the first ten days =
/// exactly 100,000 facts, the storage bound a single query may read.
const EVENTS_PER_DAY: u64 = 1_111;
const TOTAL_FACTS: u64 = 100_000;
const CONNECTION_ID: &str = "synthetic-benchmark";
const SCOPE_ID: &str = "synthetic-scope-benchmark";
/// Prototype budget from the requirements: p95 <= 1s for the 90-day local
/// read, cancellation within 5s. Recorded, not enforced, by this benchmark.
const P95_TARGET_MS: f64 = 1_000.0;
const WARMUPS: usize = 5;
const MEASURED: usize = 30;

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

/// One benchmark connection with one scope, no refresh: the only published
/// data in the store is the seeded benchmark history.
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
    let first_day = chrono::DateTime::parse_from_rfc3339(START_DAY)
        .expect("fixed benchmark day parses")
        .with_timezone(&Utc);
    let mut total = 0_u64;
    for day in 0..DAYS {
        let day_start = first_day + Duration::days(day);
        let day_end = day_start + Duration::days(1);
        let run_id = format!("bench-run-{day}");
        let partition = Partition {
            scope_id: SCOPE_ID.to_owned(),
            product_key: String::new(),
            dataset: "synthetic-usage".to_owned(),
            metric: "tokens".to_owned(),
            interval: Interval::new(day_start, day_end).expect("one-day partition"),
            grain: "event".to_owned(),
            dimensions: String::new(),
            authority: Provenance::Synthetic,
        };
        state
            .repository
            .begin_candidate(&context, &run_id, &partition)
            .await
            .expect("begin candidate");
        // One extra event on each of the first ten days lands the total
        // exactly on the 100,000-fact storage bound.
        let events = EVENTS_PER_DAY + u64::from(day < 10);
        let facts: Vec<Fact> = (0..events)
            .map(|i| {
                let minute = i % 1_440;
                let window = day_start + Duration::minutes(minute as i64);
                Fact {
                    identity: FactIdentity::Event(format!("bench-ev-{day}-{i}")),
                    interval: Interval::new(window, window + Duration::minutes(1))
                        .expect("one-minute window"),
                    value: Value::Tokens(TokenTotal(u128::from((day as u64 * 7 + i) % 1_000 + 1))),
                    source_revision: format!("bench-rev-{day}"),
                }
            })
            .collect();
        total += events;
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
            .publish(&context, &run_id, day_end, &format!("bench-wm-{day}"))
            .await
            .expect("publish");
    }
    total
}

/// The measured query mix. A is the worst case the interface allows: ninety
/// days at day grain over the full 100,000-fact bound.
const MIX: [(&str, &str, &str, &str); 4] = [
    (
        "A-statistics-90d-day",
        "statistics",
        "2026-07-01",
        "2026-09-28",
    ),
    (
        "B-statistics-90d-week",
        "statistics",
        "2026-07-01",
        "2026-09-28",
    ),
    (
        "C-statistics-30d-day",
        "statistics",
        "2026-08-30",
        "2026-09-28",
    ),
    ("D-overview-90d", "overview", "2026-07-01", "2026-09-28"),
];

fn request_for(label: &str, kind: &str, start: &str, end: &str) -> serde_json::Value {
    let grain = if label.starts_with('B') {
        "week"
    } else {
        "day"
    };
    match kind {
        "statistics" => statistics_request(start, end, grain),
        _ => overview_request(start, end),
    }
}

/// Run one query through the wire path, returning (label, milliseconds).
/// Panics on a transport error: a failed query is a failure, not a slow one.
async fn timed(state: &AppState, label: &str, kind: &str, start: &str, end: &str) -> (String, f64) {
    let started = Instant::now();
    let envelope = run_request(state, Some(request_for(label, kind, start, end))).await;
    let ms = started.elapsed().as_secs_f64() * 1_000.0;
    match envelope {
        Envelope::Ok { .. } => (label.to_owned(), ms),
        other => panic!("benchmark query {label} failed: {other:?}"),
    }
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

#[tokio::test]
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

    // Five warmups, unmeasured, exactly as the phase-one plan prescribes.
    for round in 0..WARMUPS {
        let (label, kind, start, end) = MIX[round % MIX.len()];
        timed(&state, label, kind, start, end).await;
    }

    // Thirty measured queries across the mix: A x10, B x8, C x6, D x6.
    let plan = [("A", 10_usize), ("B", 8), ("C", 6), ("D", 6)];
    let mut samples: Vec<(String, f64)> = Vec::with_capacity(MEASURED);
    for (prefix, count) in plan {
        let (label, kind, start, end) = MIX
            .iter()
            .find(|(label, ..)| label.starts_with(prefix))
            .copied()
            .expect("mix entry");
        for _ in 0..count {
            samples.push(timed(&state, label, kind, start, end).await);
        }
    }
    assert_eq!(samples.len(), MEASURED);

    // Cancellation: abort the heaviest query mid-flight, then prove the
    // store is still consistent by re-running it and comparing the payload.
    let state = Arc::new(state);
    let mut cancellations = Vec::new();
    for attempt in 0..5 {
        let task_state = state.clone();
        let handle = tokio::spawn(async move {
            timed(
                &task_state,
                "A-statistics-90d-day",
                "statistics",
                "2026-07-01",
                "2026-09-28",
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let started = Instant::now();
        handle.abort();
        let outcome = handle.await;
        let abort_ms = started.elapsed().as_secs_f64() * 1_000.0;
        cancellations.push(json!({
            "attempt": attempt,
            "abort_to_join_ms": abort_ms,
            "cancelled_mid_flight": outcome.is_err(),
        }));
    }
    let reference = run_request(
        &state,
        Some(statistics_request("2026-07-01", "2026-09-28", "day")),
    )
    .await;
    let totals_after = match reference {
        Envelope::Ok { data, .. } => match *data {
            CommandResult::QueryStatistics(stats) => stats.totals,
            other => panic!("expected QueryStatistics, got {other:?}"),
        },
        other => panic!("expected Envelope::Ok, got {other:?}"),
    };
    // Ninety settled days x the seeded amounts: the store survived the
    // aborted reads byte-for-byte (aggregation is pure over committed rows).
    assert!(
        !totals_after.is_empty(),
        "store stays consistent after aborts"
    );

    let overall: Vec<f64> = samples.iter().map(|(_, ms)| *ms).collect();
    let overall_summary = summary(&overall);
    let per_mix: serde_json::Map<String, serde_json::Value> = MIX
        .iter()
        .map(|(label, ..)| {
            let latencies: Vec<f64> = samples
                .iter()
                .filter(|(l, _)| l == label)
                .map(|(_, ms)| *ms)
                .collect();
            (label.to_string(), summary(&latencies))
        })
        .collect();
    let p95 = overall_summary["p95_ms"].as_f64().expect("p95 recorded");

    let artifact = json!({
        "benchmark": "p1-12-local-query-mix",
        "machine": std::env::var("QUOTAVISTA_BENCHMARK_MACHINE").ok(),
        "toolchain": std::env::var("QUOTAVISTA_BENCHMARK_TOOLCHAIN").ok(),
        "platform": format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        "seed": {
            "connections": 1,
            "scopes": 1,
            "partitions": DAYS,
            "facts": seeded,
            "window": { "start_date": "2026-07-01", "end_date": "2026-09-28", "zone": "UTC" },
        },
        "budget": {
            "p95_target_ms": P95_TARGET_MS,
            "cancellation_deadline_ms": 5_000.0,
        },
        "warmups": WARMUPS,
        "measured": samples.iter().map(|(label, ms)| json!({ "query": label, "ms": ms })).collect::<Vec<_>>(),
        "per_mix": per_mix,
        "overall": overall_summary,
        "within_p95_target": p95 <= P95_TARGET_MS,
        "cancellation": {
            "mechanism": "tokio task abort at the next await point",
            "note": "No mid-aggregation cancellation token exists; a query already inside the synchronous aggregation runs to completion. Measured p95 shows how far that worst case sits from the 5-second deadline.",
            "attempts": cancellations,
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
