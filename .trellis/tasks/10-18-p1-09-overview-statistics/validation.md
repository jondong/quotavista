# P1-09 validation record

## Scope and prerequisites

P1-09 implements the simulated overview and period statistics views on the P1-07 IPC composition: `get_overview` and `query_statistics` now run a bounded aggregation over published synthetic facts instead of answering `UnsupportedInPhaseOne`. Kimi shared quota (remaining ratio, consumed percentage points, refresh time) renders separately from OpenRouter Credits-style spend per scope, with independent availability, freshness, and source timestamps.

Dependencies verified with git before starting (worktree branch `jondong/p1-09-overview-statistics` off `main`):

- P1-07 merged via PR #8 (head 9188637, merge commit 34fc4c3), archived at `.trellis/tasks/archive/2026-10/10-18-p1-07-ipc` by 44a0f33.
- P1-08 archived at `.trellis/tasks/archive/2026-10/10-18-p1-08-bilingual-shell` by a54b56b.
- P1-01..P1-06 archived under `.trellis/tasks/archive/2026-10/`.

Environment: macOS 27.0.1 arm64, Rust 1.99.1, Node 24.22.0, npm 11.19.1, SQLx 0.9.0 (bundled SQLite 3.51.3), Tauri 2.x. Every fixture is synthetic: throwaway `TempDir` SQLite databases, in-repo simulated adapters, session-only secret stores, recorded change sinks. No real credential, keychain entry, provider endpoint, or application data directory was read, written, or deleted.

## What was built

- `crates/core/src/statistics.rs`: pure aggregation engine over `CalendarRange` (P1-02 helpers) and P1-03 published facts. Day/week (Monday-start) buckets clipped to the effective interval; UTC-only source buckets classified into an explicit UTC lane and excluded from reporting-zone totals with `BucketBoundaryMismatch`; per-point coverage (`Complete` / `Partial` / `Missing` with `NotYetSettled`, `OutsideLookback`, `CollectionGap`, `BucketBoundaryMismatch`); provisional flag and as-of clipping for the current period; recorded-token totals through checked `u128` sums of published facts only (snapshots never enter range totals).
- `crates/core/src/number.rs`: `TokenFact::recorded_total()` — only `Additional` subcounts are added; `Included`/`Unknown` never are; missing base yields null, never zero.
- `crates/core/src/storage.rs`: bounded read ports `snapshots(scope_ids)` and `published_partitions(scope_ids, range)` plus `StorageError::ResourceLimit`; implemented in `crates/infrastructure/src/sqlite.rs` with a 100,000-fact pre-count guard.
- `crates/providers/src/simulated.rs`: Kimi capability gains `quota_remaining` (ratio 0.70) and `quota_consumed` (30 percentage points) metrics; OpenRouter exposes no quota (explicit Unsupported); new `SimulatedRouter` routes demo connections by id so the Kimi and OpenRouter fixtures coexist in one store.
- `crates/contracts`: `StatisticsPoint` gains a `zone` field (UTC lane must not be disguised as local days); `Grain` mapped from the domain enum; `StatisticsDto`/`OverviewDto` `from_domain` mapping; regenerated `apps/desktop/src/generated/contracts.ts`.
- `apps/desktop/src-tauri/src/lib.rs` + new `apps/desktop/src-tauri/src/statistics.rs`: `get_overview`/`query_statistics` handlers (validate range via the shared `calendar_range` path, resolve scopes, read via the bounded ports, run the engine, map to DTOs, carry the store `data_revision`); demo seeding of one Kimi and one OpenRouter synthetic connection on first run; `ResourceLimit` mapped to the sealed error.
- `apps/desktop/src/screens/OverviewScreen.tsx` and `StatisticsScreen.tsx`: real IPC-backed views. Overview shows per-scope Kimi quota vs OpenRouter spend with refreshed-at and source-as-of timestamps and stale badges. Statistics offers all eight range presets plus an explicit custom start/end date pair (future dates blocked by a `max=today` ceiling and backend validation), day/week grain, explicit range dates and zone, provisional marker, per-zone lanes, excluded-scope list with reasons, per-plan rows with native units, an explanation instead of an all-zero line for missing history, and obsolete-response/stale-revision protection via the P1-07 `RevisionTracker` watermarks. `i18n/index.tsx` gains exact string-math `formatPercent`; both catalogs carry the 99 keys.

## Validation status

Every command was run locally on this branch; results below are the real observed output.

| Command | Result |
| --- | --- |
| `./scripts/check-core-boundaries.sh` | PASS — "quotavista-core calendar-only dependencies: boundary OK" |
| `./scripts/test-check-core-boundaries.sh` | PASS — "all boundary-gate regression checks passed" |
| `cargo fmt --all -- --check` | PASS (after one `cargo fmt --all` normalization of newly written code) |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS — no warnings |
| `cargo test --locked --workspace` | PASS — 164 passed, 0 failed, 1 ignored (pre-existing native-keychain test): contracts 16, core 29, desktop 27, infrastructure 66 (+1 ignored), providers 13, contract doc-tests 13 |
| `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` | PASS — "TypeScript contract is current" |
| `cargo build --locked -p quotavista-desktop` | PASS |
| `npm ci` (apps/desktop) | PASS |
| `npm run lint` | PASS |
| `npm run i18n:check` | PASS — "Catalog parity OK: 99 keys, placeholders matching in both catalogs" |
| `npm test` | PASS — 5 test files, 48 tests, no unhandled errors |
| `npm run typecheck` | PASS |
| `npm run build` | PASS — vite production build |
| `git diff --check` | PASS — no whitespace errors |

## Synthetic acceptance evidence verified

| PRD criterion | Fixture and required result |
| --- | --- |
| Known-result totals, distinct units, no double counting | `quotavista-desktop` test `statistics_sums_the_known_fixture_totals_without_double_counting`: seeded Kimi (token total 1,234,567,890,123) and OpenRouter (9,876,543,210,987) buckets sum to the exact expected per-scope totals, range total equals their sum, current snapshots with the same values do not change the range total, and plan rows keep distinct units (tokens, ratio, percentage points, USD money). Core tests `recorded_total_adds_only_additional_subcounts` (only `Additional` subcounts are added; `Included`/`Unknown` never are) and `snapshots_never_enter_range_totals` pin the anti-double-count rules. |
| Missing Kimi tokens excluded from Recorded tokens, native quota visible, unavailable history never zero | `missing_kimi_history_excludes_the_scope_from_recorded_tokens` (NoHistory: Kimi scope excluded with `NoData`, range total equals the OpenRouter-only sum); `missing_kimi_tokens_keep_the_native_quota_visible_in_the_overview` (MissingTokens: current tokens `Missing`+`NoData`, quota ratio still `Available` with a source timestamp); core `empty_effective_interval_reports_missing_not_zero` and frontend `shows the empty explanation instead of a zero line when nothing was recorded` / `shows unavailable buckets as unknown, never as zero`. |
| Explicit dates/zone, 366-day and future-date rules, obsolete responses ignored, selection preserved across language switches | `statistics_rejects_future_reversed_and_overlong_ranges` (all three rejected with the sealed validation error before any read); frontend `shows the explicit range dates, zone, and provisional marker`, `reveals explicit custom date inputs with a future-date ceiling` (`max=today`), `discards a response that is older than the observed revision`, `keeps the previous view when a reload fails`, and the App-level `switching language preserves the current screen, filter, and nonsecret input` / `re-translates the overview labels after a language switch` (screen state is not keyed by locale). |
| UTC-only lane, partial edge buckets, current-day clipped at as-of and provisional | `a_non_utc_zone_keeps_utc_buckets_in_their_own_lane` (America/New_York reporting: null total, `BucketBoundaryMismatch` exclusions, three UTC-lane points with exact sums, local-lane points missing rather than zero); core `utc_zone_week_grain_keeps_clipped_edge_partial` (clipped edge week stays `Partial`/`BucketBoundaryMismatch`, only fully-contained facts summed); `utc_zone_day_grain_sums_exact_known_totals` (current day clipped at as-of, marked `Missing`/`NotYetSettled`, `provisional: true`). |

## Composition wiring

`get_overview` and `query_statistics` share the single `run_request` decode/validate/dispatch path from P1-07; the handlers recompute the validated calendar range, resolve requested scopes, read through the two new bounded ports, and map the engine output through `StatisticsDto::from_domain`/`OverviewDto::from_domain` with the store `data_revision`, so the renderer's watermarks and obsolete-response checks apply unchanged. The `StatisticsPoint.zone` contract addition is additive (new required field on a DTO the backend always constructs); `cargo run -p quotavista-contracts --bin generate-contracts -- --check` confirms the generated TypeScript matches.

## Review boundaries and open release work

- Out of scope by design and not claimed: live usage totals, estimation from balance/quota differences, provider completeness claims, a retention/backfill engine, final chart polish, and everything belonging to P1-10 (connections/settings) or P1-11 (desktop lifecycle).
- The 100,000-fact statistics throughput benchmark (P95 ≤ 1 s) remains pending for P1-12; the bound itself is enforced and tested (`statistics_read_refuses_an_unbounded_fact_scan`).
- Native GUI smoke of the seeded demo window was NOT run in this round (no display session driven); the demo seeding path is covered only by unit-level composition tests. This check is recorded as pending; no unobserved GUI behavior is claimed.
- Native keychain tests remain ignored (as in P1-05/P1-07); secrets in all fixtures are session-only fakes.
- `update_settings` and the other P1-10 commands still return the typed `UnsupportedInPhaseOne` error; the reporting zone is therefore the UTC prototype constant.

## Stale link fixes recorded

The task's `prd.md`, `design.md`, and `implement.md` linked `../10-18-p1-07-ipc/prd.md` and `../10-18-p1-08-bilingual-shell/prd.md`; both now point at `../archive/2026-10/…`. `docs/phase-one-plan.md` lines 27–28 carry the same stale P1-07/P1-08 links but were left untouched (not lines this task otherwise edits; sibling tasks share the file).

## Recoveries worth noting

- `sqlx` 0.9 rejects dynamically built SQL strings, so the bounded reads use static per-scope queries inside one read transaction (scope count is contract-bounded at 100).
- Coverage rows must be lane-qualified by `source_zone`: without the `row.zone == zone.name()` check a UTC coverage row marked local-zone buckets complete even though the UTC facts were excluded from that lane. The `non_utc_zone_keeps_utc_buckets_in_a_separate_lane` core test caught this.
- `formatPercent` shifts the decimal point with pure string arithmetic (`0.70` → `70%`, `1` → `100%`) so ratios never round through a floating-point `Number`.
