# P1-12 validation record

## Scope and prerequisites

P1-12 validates the integrated Phase 1 prototype — recovery, bilingual flows,
and local query budgets — as prototype evidence only. It closes no release
acceptance criterion. Baselines: requirements 0.9, architecture 0.4,
development plan, and traceability. Predecessors
[P1-09](../archive/2026-10/10-18-p1-09-overview-statistics/validation.md),
[P1-10](../archive/2026-10/10-18-p1-10-connections-settings/validation.md),
[P1-11](../archive/2026-10/10-18-p1-11-desktop-lifecycle/validation.md) were
verified archived with their own validation records before this task started.

Everything below ran on the task branch `jondong/p1-12-integration` on
2026-10-21, host Apple M6, 32 GB, macOS 27.0.1 (26A434), arm64, toolchain
rustc 1.99.1 (58cf8f9c3 2026-10-13), Node 24, npm 11.19.1. No real credential,
provider account, production database, or application data directory was
touched: every fixture uses a throwaway temporary directory, the session-only
secret store, or disposable `quotavista-test-` Keychain entries (cleanup
evidence below).

## Design decisions recorded during implementation

- The design says to use the task-specific tests already created rather than
  duplicating them. The audit found the four-screen bilingual workflow,
  failure, recovery, and secret-isolation behavior already covered by the
  P1-09/P1-10/P1-11 suites (cited per criterion below), so P1-12 adds only the
  two missing pieces: the deferred 100,000-fact local query benchmark and one
  P1-10 review follow-up test (refresh-interval cycle).
- The benchmark lives in `apps/desktop/src-tauri/src/benchmark.rs` as an
  env-gated `#[cfg(test)]` module (`QUOTAVISTA_BENCHMARK=1`), so the default
  `cargo test --locked --workspace` stays fast and CI is unaffected. Seeding
  goes through the repository write path (candidate → staged pages → complete
  → publish); measured queries go through the same validated `run_request`
  wire path the renderer uses. The artifact is written outside application
  state and the committed copy is `benchmark.json` in this directory.
- Cancellation evidence is measured honestly: the harness runs on a
  multi-thread runtime with start synchronization, rejects attempts that
  finished before the abort, asserts the join resolves as a real task
  cancellation inside the enforced 5,000 ms deadline, and proves store
  consistency by comparing the exact totals and series values before and
  after the aborts. The 5-second query-work bound is recorded as unverified
  in the artifact, with its owning follow-up under "Pending and not claimed".

## PRD criterion evidence

Criterion, method, result.

### Criterion 1 — required automated checks pass reproducibly; failures and limitations have owning follow-ups; no real credentials required

- Every documented check passes locally with exit 0 (full list and output in
  "Validation commands" below): both boundary scripts, `cargo fmt --all --
  --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace` (221 passed, 0 failed, 1 ignored native
  smoke), `cargo run -p quotavista-contracts --bin generate-contracts --
  --check`, and in `apps/desktop`: `npm ci`, `npm run lint`,
  `npm run i18n:check`, `npm test` (82 tests in 8 files), `npm run typecheck`,
  `npm run build`.
- CI was deliberately not triggered: the coordinator instructed that this
  branch is committed locally only (no push, no PR). CI runs the same
  commands from the same lockfiles; the only CI-visible change from this task
  is one fast e2e test plus the env-gated benchmark, which skips in
  milliseconds without `QUOTAVISTA_BENCHMARK=1`.
- No check failure occurred. The one limitation found (no mid-aggregation
  cancellation token) has an owning follow-up recorded under "Pending and not
  claimed".

### Criterion 2 — benchmark artifact records actual timings against the 1-second P95 target and 5-second cancellation bound; misses stay open

The committed artifact is [benchmark.json](benchmark.json), produced by
`benchmark::statistics_query_mix_benchmark`
(`apps/desktop/src-tauri/src/benchmark.rs`):

- Seed: one synthetic connection/scope holding fourteen weekly partitions
  (Mon 2026-06-29 .. Mon 2026-10-05) of event-grain history whose facts each
  span exactly one UTC day, covering ninety settled days
  (2026-07-01..2026-09-28) — exactly 100,000 facts, the storage bound a single
  query may read (`MAX_STATISTICS_FACTS`). Every fact is compatible with the
  UTC reporting zone, so each measured query aggregates the real workload; the
  artifact records this under `seed.fact_shape`.
- Correctness gate: before any timing, every mix must reproduce the known
  exact values computed from the seed — range totals (399,570 tokens over 90
  days; 133,170 over the 30-day query), every series point's exact value and
  coverage row (day grain: 90 Complete points; week grain: 12 interior
  Complete plus 2 clipped edges Partial/`BucketBoundaryMismatch`), and an
  empty exclusion set. Recorded per mix under `correctness` in the artifact.
- Protocol: 5 warmups, then 30 measured queries through `run_request`:
  A statistics 90-day/day-grain ×10, B statistics 90-day/week-grain ×8,
  C statistics 30-day/day-grain ×6, D overview 90-day ×6. Failures: 0.
- Result (values copied from the artifact, one decimal): overall p50 845.7 ms,
  **p95 866.6 ms, max 868.6 ms against the 1,000 ms P95 target — within
  target** (`within_p95_target: true`). Per mix (p50/p95/max ms): A
  849.5/868.6/868.6, B 784.9/799.8/799.8, C 318.0/318.8/318.8, D
  858.7/866.6/866.6. Mixes A/B/D are dominated by the bounded 100k-row read
  plus the real aggregation of 100,000 compatible facts; mix C reads a third
  of the range. The target is met on this machine; it is recorded, not
  redefined, and remains a per-machine measurement, not a cross-platform
  guarantee.
- Cancellation: five valid attempts on a multi-thread runtime abort the
  heaviest query task 100 ms after a synchronized start; no attempt finished
  before its abort (such attempts are rejected, not counted), every abort
  lands mid-flight and the join resolves as a real task cancellation
  (`JoinError::is_cancelled`) in 0.7–2.3 ms (max 2.3 ms) under the enforced
  5,000 ms deadline, and no cancelled query ran to completion. A fresh query
  afterwards reproduces the exact pre-abort totals and every series value.
  The artifact additionally records `query_work_cancellation_bound` as
  **unverified**: abort-to-join measures consumer-facing responsiveness, and
  an abort landing inside the synchronous aggregation cannot stop that work —
  the owning follow-up is listed under "Pending and not claimed".

### Criterion 3 — evidence distinguishes implemented behavior, synthetic tests, native checks, and provider/release work; A01–A04 and G01/G02 remain unverified

- The traceability update (`docs/traceability.md`, "P1-12 prototype evidence")
  labels every line as implemented-and-tested, synthetic-fixture evidence,
  native-host evidence, or deferred provider/release work, and links this
  record. A01–A04 keep their Pending status; G01/G02 (and G04) stay open.
- Native evidence actually collected on this host (macOS): the previously
  pending Keychain smoke check `macos_keychain_synthetic_roundtrip`
  (`crates/infrastructure/src/secrets/tests.rs`, `#[ignore]`d by default) ran
  green against the real macOS Keychain using a disposable
  `quotavista-test-p1-05-3047` service: create/read/rotate/delete of two
  synthetic entries, deletion re-run, both entries reading back
  `NeedsCredentials`. Cleanup verified independently afterwards:
  `security find-generic-password -s quotavista-test-p1-05-3047` reports "The
  specified item could not be found in the keychain" and
  `security dump-keychain | grep -c quotavista-test` prints 0 — no
  `quotavista-test-` entries remain.
- Linux native evidence (Secret Service, tray) was not collected; this is a
  macOS-only host and that evidence keeps its pending status.
- Also fixed while updating traceability (P1-10 review follow-up b): the
  `## Release evidence rules` heading that had been joined onto the end of the
  P1-10 A25 bullet now stands on its own line with blank lines around it, and
  the three stale predecessor links (P1-09/P1-10/P1-11 validation records)
  were repointed to their archived locations under
  `.trellis/tasks/archive/2026-10/`, keeping A24's "required local links
  resolve" true.

### Criterion 4 — both locale flows preserve exact statistics, reporting zones, selections, and safe recovery; no critical prototype acceptance defect remains unowned

- Locale selection and switching: `App.test.tsx` — `renders English by default
  for a non-Chinese system locale`, `chooses zh-CN on first launch for an
  explicitly Simplified Chinese system locale`, `keeps English for a
  Traditional Chinese system locale`, `applies an explicitly saved backend
  locale over system detection`, `keeps first-launch system detection while
  the settings row is untouched`, `switching language preserves the current
  screen, filter, and nonsecret input`, `re-translates the overview labels
  after a language switch`, `still switches language in memory when the
  persisted write fails`; `ConnectionsScreen.test.tsx` — `preserves the alias
  draft across a language switch but clears the secret`;
  `SettingsScreen.test.tsx` — `preserves non-secret form drafts across a
  language switch`, `an older in-flight locale write never overwrites a newer
  committed choice`.
- Exact statistics transport (asserted in the English-forced statistics
  suite): `StatisticsScreen.test.tsx` forces English
  (`quotavista.ui_locale = "en"` in `beforeEach`) and asserts `renders the
  exact backend totals and per-point values`, `shows the explicit range dates,
  zone, and provisional marker`, `shows unavailable buckets as unknown, never
  as zero`, `lists excluded scopes with their reason`, `renders per-plan rows
  with native units and percentages`, `marks partial buckets instead of
  presenting them as final`. Both-locale coverage is at the formatter level:
  `i18n.test.ts` — `groups token counts exactly
  without Number rounding`, `keeps every fractional digit of money amounts`,
  `formats the same instant in the independent reporting zone` — plus the
  locale-switch state-preservation tests above. The full zh-CN
  failure-scenario matrix (exact values, ranges, zones, and partial buckets
  rendered in zh-CN) is not asserted today; it stays pending with the P1-13
  native/runtime checks (see "Pending and not claimed").
- Catalog parity: `npm run i18n:check` passes (en/zh-CN key parity gate).
- Safe recovery across restart and lifecycle: `connections_tests.rs` —
  `explicit_settings_survive_restart`, `failed_cleanup_survives_restart_and_retries`,
  `session_only_connection_needs_credentials_after_restart`,
  `refresh_interval_cycle_survives_restart_and_blocks_wakes_when_off` (new,
  P1-10 review follow-up a: the full Off → 1 min → 5 min → 15 min → Off cycle
  persists the interval and the settings revision across a close-and-rebuild
  of the whole composition after every save, and wakes stay blocked —
  `!permitted`, nothing launched — whenever the interval is Off);
  `lifecycle.rs` tests — `refresh_off_never_initiates_an_implicit_query_on_wake`,
  `quit_leaves_committed_data_readable_after_a_restart`,
  `a_corrupt_lifecycle_journal_blocks_the_composition_before_scheduling`;
  `lib.rs` tests — `a_degraded_composition_reports_the_credential_store_error`,
  `a_refused_commit_keeps_the_cached_data_and_announces_no_revision`.
- No-tray and notification honesty: `DesktopIntegration.test.tsx` — `explains
  that closing the window exits when no tray was detected`, `reports a queued
  notification as unconfirmed rather than accepted`, `explains a failed
  transport without pretending the notification went out`;
  `lifecycle.rs` — `closing_the_main_window_exits_when_no_tray_was_detected`.
- Secret isolation across IPC and storage: `connections_tests.rs` —
  `secrets_never_leave_the_store`, `invalid_aliases_and_secrets_fail_safely`;
  `lib.rs` — `only_connect_and_replace_accept_secret_input`,
  `secret_errors_keep_locked_and_unavailable_distinct`,
  `oversized_secret_is_rejected_while_a_valid_credential_decodes`;
  `ipc.test.ts` revision-watermark suite (12 tests).
- No new defect was found during this validation; the single gap that
  surfaced (cancellation token) is owned below.

## Validation commands (all run 2026-10-21 on this branch, all exit 0)

```text
./scripts/check-core-boundaries.sh                            rc=0
./scripts/test-check-core-boundaries.sh                       rc=0
cargo fmt --all -- --check                                    rc=0
cargo clippy --locked --workspace --all-targets -- -D warnings rc=0
cargo test --locked --workspace                               rc=0 (221 passed, 0 failed, 1 ignored)
cargo run -p quotavista-contracts --bin generate-contracts -- --check  rc=0 (contract is current)
(cd apps/desktop && npm ci)                                   rc=0
(cd apps/desktop && npm run lint)                             rc=0
(cd apps/desktop && npm run i18n:check)                       rc=0
(cd apps/desktop && npm test)                                 rc=0 (82 passed, 8 files)
(cd apps/desktop && npm run typecheck)                        rc=0
(cd apps/desktop && npm run build)                            rc=0
```

Benchmark and native smoke (manually gated, both rc=0):

```text
QUOTAVISTA_BENCHMARK=1 cargo test --locked -p quotavista-desktop statistics_query_mix_benchmark -- --nocapture
cargo test --locked -p quotavista-infrastructure macos_keychain_synthetic_roundtrip -- --ignored --nocapture
```

`git diff --check` is clean.

## Pending and not claimed

- **Mid-aggregation cancellation token — owned follow-up.** Architecture's
  bounded-work table asks long local aggregations to check a cancellation
  token between bounded batches; the aggregation engine is currently
  synchronous with no token, so the artifact records
  `query_work_cancellation_bound` as unverified. What is verified: consumer-
  facing cancellation — aborting the in-flight query task resolves the join
  as a real cancellation in 0.7–2.3 ms under the enforced 5,000 ms deadline,
  and a query already inside the synchronous aggregation would run to
  completion (max 868.6 ms for the worst legal query here, vs the 5-second
  failure bound). Owning follow-up: Phase 2 bounded-work hardening
  (cooperative cancellation token through `query_statistics` into
  `statistics::aggregate`).
- A01–A04 (provider integrations) and G01/G02 remain unverified by design;
  G04 stays open. Linux native evidence (Secret Service, tray behavior) was
  not collected on this macOS-only host and keeps its pending status.
- **zh-CN failure-scenario matrix — owned follow-up.** The exact statistics
  assertions (exact totals and per-point values, explicit range dates and
  zones, partial buckets, excluded scopes, failed reloads) run in the
  English-forced `StatisticsScreen.test.tsx` suite; both-locale coverage is
  formatter-level (`i18n.test.ts`) plus locale-switch state preservation.
  Rendering the full failure-scenario matrix in zh-CN stays pending with the
  P1-13 native/runtime checks.
- CI has not run on this branch (local commits only, per the coordinator);
  the green local battery above uses the same commands CI runs.
- Prototype benchmark results are single-machine measurements; they are not
  cross-platform performance guarantees and no release acceptance is claimed.
