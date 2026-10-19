# P1-02 design

## Approach and boundaries

Domain primitives stay serialization-independent. Place transport types at a dedicated boundary with explicit mappings and one contract-generation strategy. Return exact integer/decimal strings; carry native units and source bucket boundaries through every layer.

## Implementation decisions

- `crates/core/src/number.rs` uses a private `u128` coefficient and scale for exact, nonnegative decimals: at most 28 significant digits and 18 fractional places. Ingestion rejects excess precision before normalization; arithmetic is checked and never rounds. Token facts use `u64`, totals use checked `u128`, and nullable subcounts retain explicit inclusion relations.
- `crates/core/src/calendar.rs` uses `chrono` without clock/serialization features and `chrono-tz` for IANA boundaries. Inclusive dates produce a requested half-open interval; effective time is clipped to one injected clock read. At exactly local midnight the effective interval is absent, not a fabricated zero or invalid request. Actual intervals retain `start < end`. Ambiguous or nonexistent local midnight is an explicit boundary error, never guessed. `monday` returns the local calendar week anchor; source buckets must fit wholly inside effective boundaries.
- Core has no serde or TypeScript derives. `crates/contracts` uses serde for validation/transport and `ts-rs` 12.0.1 for deterministic TypeScript output. String newtypes carry exact tokens, decimals, revisions, dates, instants, zones, and local IDs. Unsupported serde-attribute diagnostics from `ts-rs` are not suppressed; serialization/validation tests independently verify the wire representation. TypeScript aliases describe wire shapes, not runtime validation guarantees.
- Requests accept exactly the reviewed eleven commands, a contract version, request ID, and typed arguments. Secret inputs have no Debug or Serialize implementation, reject blank/CR-LF/oversized input, and preserve accepted bytes for the future provider adapter. Response errors are constructed from fixed safe messages; no raw exception or provider-body field exists. Runtime authorization, entity lookup, revision conflict checks, body limits, and command registration remain P1-07 work.
- PR #2 follow-up seals SafeError taxonomy and MetricDto unit/value pairs. Checked serde remote helpers retain duplicate/unknown-field rejection while enforcing the same invariants as construction. `internal_error` identifies local mapping failures without blaming a provider or the caller. Tokens remain canonical u128 strings; decimal and ratio limits remain unchanged.
- Alert classification is an explicit core `QueryKind` mapped to the required transport `query_kind`, not an unverified bare boolean. Historical, delayed-historical, stale-cache and unvalidated results cannot trigger alerts even if their observed freshness is current. `NotYetSettled` excludes eligibility; `SyncPending` alone does not. Domain and DTO checks reject available/null/no-value contradictions.
- Final re-review seals the alert flag and all of its state inputs behind read-only accessors. One DTO eligibility predicate is used by construction and checked decoding; parity tests compare it with core rules across 600 state combinations. A stale payload claiming true, a current eligible payload claiming false, or an absent/unknown query classification rejects. SafeError retry timestamps are also read-only.
- Overview has a grain-free `OverviewRangeInput`; statistics retains required day/week grain. `SourceLabel` bounds native-unit names, source period keys, and authorization categories to 128 Unicode code points without truncation or control characters. Generated TS and compile-only request fixtures follow these source contracts.
- `scripts/check-core-boundaries.sh` permits only the pure calendar direct dependencies, examining all targets/features and all dependency kinds. Regression fixtures reject ordinary, optional, and inactive-target forbidden dependencies and Cargo failures. No persistence schema, migration, or database is involved.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-01](../archive/2026-10/10-18-p1-01-workspace/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No live endpoint schema, universal token availability assumption, complete statistics engine, or provider authentication research.
