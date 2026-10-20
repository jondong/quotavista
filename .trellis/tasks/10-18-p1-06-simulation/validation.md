# P1-06 validation record

## Scope and prerequisites

Backend scope only. P1-06 adds the refresh port in `quotavista_core::provider` (request/response models, typed `ProviderError`, persisted `Cooldowns`, injectable `RefreshClock`), deterministic simulated providers in `quotavista_providers::simulated` (eight scenarios over the two synthetic capability fixtures), and the bounded refresh coordinator in `quotavista_infrastructure::refresh` (`RefreshCoordinator`). No contracts, generated TypeScript, frontend, desktop command, migration, or credential change is included, because the desktop composition tasks wire these pieces together.

Prerequisite confirmed before starting: P1-04 is merged ([PR #5](https://github.com/jondong/quotavista/pull/5), merge commit `46f8db9`) and archived at `.trellis/tasks/archive/2026-10/10-18-p1-04-recovery-fences` by archive commit `a38d2b4`, which is also where this branch starts from `main`.

Every fixture is synthetic and throwaway: `tempfile::TempDir` holds the SQLite database and the durable store; the durable store is an in-memory map behind the `DurableStore` port; the clock is a manually advanced fake; the provider is the in-repo simulated adapter. The `quotavista-providers` crate has no network dependency, no HTTP client, and no real endpoint or credential assumption anywhere in this change. No real application data directory was resolved, no OS keychain or native secret store was opened, and no production, business, or personal database was connected, not even for reads.

Activation was recorded with `python3 .trellis/scripts/task.py start 10-18-p1-06-simulation` after reading its `--help` output. The script reported degraded mode because no session identity is available in this worker, so the active-task pointer stayed in gitignored `.trellis/.runtime/` and only `task.json` changed.

Environment: macOS 27.0.1 arm64, Rust 1.99.1 (workspace `rust-toolchain`), SQLx 0.9.0 with bundled SQLite 3.51.3.

## Validation status

All required local validation passed. The pull request must still show the CI jobs green on the latest pushed commit before the branch is considered mergeable; CI compilation is not native platform evidence.

Commands run in this worktree, in order:

- `cargo fmt --all` then `cargo fmt --all -- --check` - passed with no diff.
- `./scripts/check-core-boundaries.sh` - passed: `quotavista-core` keeps calendar-only dependencies; the new `provider` module adds no crate dependency (chrono was already a core dependency) and no serialization crate.
- `./scripts/test-check-core-boundaries.sh` - passed: `all boundary-gate regression checks passed`.
- `cargo clippy --locked --workspace --exclude quotavista-desktop --all-targets -- -D warnings` - passed with zero errors (two unused imports in the new test module were removed during the run). The only warnings emitted are the pre-existing ts-rs `failed to parse serde attribute` notes from `quotavista-contracts`, unrelated to this change.
- `cargo test --locked --workspace --exclude quotavista-desktop` - passed, 96 tests, 0 failed: `quotavista-infrastructure` 46 (11 SQLite adapter fixtures, 3 durable-store fixtures, 23 lifecycle recovery fixtures, 1 scaffold test, 8 refresh coordinator fixtures), `quotavista-core` 13 (11 existing plus 2 provider-port/cooldown fixtures), `quotavista-providers` 8 (1 scaffold test plus 7 simulated-adapter scenario fixtures), `quotavista-contracts` integration 16 and doc-tests 13.
- `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` - passed: `TypeScript contract is current`. No DTO changed, so the generated contract and the desktop type-check input are untouched.
- Desktop `npm` checks were not run locally: this change ships no frontend, contract, or generated file, so the desktop suite has no input that differs from `main`; CI runs it on the pull request regardless.
- `git diff --check` - passed with no whitespace or conflict-marker findings.

- PR CI: [PR #6](https://github.com/jondong/quotavista/pull/6) targets `main` from this task branch; implementation commit `ea0523c`. Both jobs passed on that commit — `workspace-checks` in 1m28s and `macos-desktop` in 5m36s — and the result is recorded on the pull request. A passing CI run is not native platform evidence. This task remains in review and is not archived.

## Synthetic acceptance evidence verified

| PRD criterion | Fixture and required result |
| --- | --- |
| Demo mode cannot issue provider requests; fixtures contain synthetic identities and explicit simulated provenance | The `quotavista-providers` manifest has no network dependency and the simulated adapter is constructed purely from in-memory capability fixtures; `manual_refresh_publishes_history_and_commits_current` asserts every committed snapshot carries `Provenance::Synthetic` and the synthetic provider identity; `nominal_fixtures_are_synthetic_and_available` asserts the fixture identities and values |
| Refresh failure for one dataset/provider retains committed values and does not clear another dataset error | `failing_dataset_keeps_committed_values_and_errors_isolated` (a run whose token dataset returns `PermissionRequired` while the spend dataset succeeds keeps the previously committed token snapshot at the same value, sets health `PermissionRequired` for `shared-pool`, and leaves `topups` at `Success` in the same job) |
| Automatic refresh Off produces no implicit launch/wake/recovery query; manual requests coalesce and respect cooldown; cancellation releases concurrency slots | `automatic_refresh_off_launches_nothing_and_on_respects_interval` (`refresh_minutes: None` refuses every tick admission, creates no job, and issues no provider query; `Some(5)` admits, publishes, then refuses an immediate tick and admits again after six fake minutes); `manual_requests_coalesce_and_respect_persisted_cooldowns` (a second manual request for a live job coalesces; a rate-limited run persists `cooldowns.v1` durably before adopting the boundary, admission is `CooldownBlocked` one fake minute before expiry and admits after); `concurrency_is_two_and_cancellation_frees_the_slot` (three admissions run two and queue one; cancelling a live job immediately frees the slot and the pending job is promoted) |
| Late synthetic responses are fenced after disable/delete; partial history stays separate from published totals and cannot claim completeness | `late_responses_are_fenced_after_disable_and_deletion` (a job whose connection is disabled or moved to `deletion_pending` between the provider call and the commit is fenced by the generation re-check inside the write transaction and ends `Fenced` without overwriting committed values); `incomplete_history_keeps_published_partition_untouched` (a stream that never attests completeness is cut off at the 100-page bound, ends `Completed { history: Incomplete }`, leaves the published partition absent, and keeps the partial candidate separately addressable under its run id) |

The eight coordinator fixtures, seven adapter fixtures, and two provider-port fixtures also cover the boundaries the PRD names but does not enumerate individually: the zero/unknown/stale-upstream/missing-token current-value scenarios (`zero_unknown_stale_and_missing_scenarios_stay_explicit`), terminal-cursor pagination with an unknown-cursor rejection (`complete_history_is_three_utc_day_buckets`, `unknown_cursor_is_invalid_response`), cooldown records that only move later and fail closed on corruption (`extend_never_shortens_and_drops_expired_entries`, `cooldowns_round_trip_and_fail_closed`), and the adapter/storage partition-shape guard (`adapter_partition_shape_is_shared_with_storage`).

## Review boundaries and open release work

This task establishes deterministic simulated providers and refresh coordination; it does not complete the release criteria it contributes to (A05, A06, A07, A16, A17, A18, A19, A22 remain Pending as prototype contributions).

- No real provider, endpoint, or credential is involved anywhere; capability values are invented fixtures, and the Kimi/OpenRouter names describe simulated accounts only.
- The coordinator is not composed into desktop startup, the IPC command set, or a background scheduler loop; P1-07, P1-10, and P1-11 do the composition. `tick`, `request_manual`, `cancel`, and `pump` are explicit calls, so "no implicit wake" is a property of this API surface, not yet of a shipped process.
- The last-completion map used for interval due-ness is in-memory: a restart forgets it and re-admits on the next due tick. Persisted cooldowns, by contrast, survive restart because they are written through `DurableStore`.
- Cooldown boundaries are keyed by connection id and hold no credential, alias, or usage data; the simulated `Retry-After` is a fixed 30-minute constant, not tuned retry behavior (explicitly out of scope).
- A single coordinator instance is assumed; cross-process scheduling exclusion arrives with the desktop composition work.
- No Linux or Windows native run was performed by this worker; CI provides Linux compilation and test evidence, and neither is native platform evidence.
