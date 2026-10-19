# P1-03 validation record

## Scope and prerequisites

This task adds backend-only SQLite persistence and candidate publication. It does not connect desktop startup to a database or change frontend behavior. All fixtures use synthetic identifiers and SQLite files inside `tempfile::TempDir`; no real application, production, business, or personal database is opened, even for reads.

P1-02 was confirmed MERGED using `gh pr view 2 --json state,mergedAt,mergeCommit`: merge commit `70bf77fdd7634267407117f10912cfe8d251ceff`. Its archive exists at `.trellis/tasks/archive/2026-10/10-18-p1-02-contracts`. The branch starts from the subsequent archive commit `0df6897`. After checking `task.py start --help`, activation succeeded with `python3 .trellis/scripts/task.py start 10-18-p1-03-sqlite` with a worktree-local context selected; runtime pointer values were not committed. No task was archived and no nested agents were dispatched.

Environment: macOS arm64, Rust `1.99.1`, Node `24.22.0`, npm `11.19.1`. SQLx is pinned to `0.9.0`; `libsqlite3-sys = 0.37.0` bundles SQLite `3.51.3`. The adapter and fixtures assert the linked library version using `SELECT sqlite_version()`, not the machine's SQLite CLI.

## Validation status

All local required validation passed, including the final rerun after the alias-control-character guard/assertions. The PR must still pass both GitHub CI jobs on its latest pushed commit before handoff; the PR checks and final report are the CI record.

Already passed:

- `./scripts/check-core-boundaries.sh`: core contains only its permitted calendar/domain dependencies.
- `./scripts/test-check-core-boundaries.sh`: all five regression cases pass, including optional/inactive-target forbidden dependencies and failed Cargo metadata.
- `python3 .trellis/scripts/task.py validate 10-18-p1-03-sqlite`: five implementation and seven check context entries valid.
- `git diff --check` and `git diff --cached --check`: no whitespace errors at the time of review.
- Active LSP probes on the five touched Rust paths: four confirmed clean, one timed out; Cargo validation, not the incomplete probe, is the required final type-check evidence.

An initial infrastructure test compile failed because SQLx 0.9 requires static SQL strings and `SqlStr` in synthetic migration constructors. The test helpers were corrected to accept `&'static str` and use `SqlStr::from_static`. No SQL safety bypass or dependency downgrade was added. The adapter itself compiled before those test-only fixes.

Final validation commands and results:

- Passed: `cargo fmt --all -- --check`.
- Passed: `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- Passed: `cargo test --locked --workspace`: 40 unit/integration tests (including 11 SQLite fixtures plus the synthetic scaffold test) and 13 compile-fail doctests, zero failures.
- Passed: `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check`: TypeScript contract is current.
- Passed: `cargo build --locked -p quotavista-desktop`: unsigned debug build, no bundling or native runtime claim.
- Passed in `apps/desktop`: pinned npm `11.19.1` assertion, `npm ci` (0 vulnerabilities), `npm run lint`, `npm run typecheck`, `npm run build`.
- Passed: `cargo clippy --locked --workspace --exclude quotavista-desktop --all-targets -- -D warnings` and `cargo test --locked --workspace --exclude quotavista-desktop`: the explicit Linux CI variant, run locally on macOS; 39 unit/integration tests plus 13 compile-fail doctests, zero failures. This is not native Linux runtime evidence.
- Passed: `cargo test --locked -p quotavista-infrastructure`: all 12 tests (11 SQLite fixtures and the existing scaffold test), zero failures. This includes SQLx migration creation/reopen/checksum/fault validation; no external SQLite CLI or live database is used.
- Passed: both full-workspace Clippy/tests and dedicated infrastructure tests were rerun after the alias guard/assertions. The full workspace still has 40 unit/integration tests plus 13 compile-fail doctests.
- Pending: PR CI on the final pushed commit.

The existing `ts-rs` macro emits warnings about unsupported serde attributes; the generated contract drift check and compile-fail contract tests pass. `npm ci` reports the existing ESLint deprecation and unapproved dependency install-script warnings; audit reports zero vulnerabilities and the frontend build passes. No unrelated dependency or frontend changes were made.

## Synthetic acceptance evidence verified

| Task criterion | Fixture methods and required result |
| --- | --- |
| Fresh migration, reopen, exact values, failed writes preserve success time | `fresh_migration_reopen_settings_and_exact_values` checks settings CAS, reopen, original observation/source/success times, u64/u128 boundaries, exact money/ratio/percentage-point/native values, TEXT storage, integrity and FK checks. `failed_snapshot_rolls_back_value_success_time_and_revision` injects a trigger and invalid product FK and requires unchanged committed state/revision. |
| Candidate page/cursor atomicity and interrupted candidates | `candidate_page_cursor_rollback_restart_and_partial_replacement` injects failure at cursor advance after staging, checks rollback/reopen and unchanged publication, rejects incomplete publication and stale/repeated cursors, then resumes and completes. |
| Complete replacement, idempotency, stable event identity | Candidate/replacement, publication-failure, event-identity, bucket-key, and concurrent-writer fixtures check removed facts, authoritative empty partitions, duplicate upserts, corrected timestamps, repeat publication, atomic metadata, and cross-partition/overlap rejection. |
| Pool settings, bounded busy errors, migration failure, linked version | `pooled_settings_linked_version_and_bounded_busy_error` holds all three readers plus the writer simultaneously and checks each connection's WAL/FK/busy/FULL settings and linked version, then holds an external write lock and requires a bounded safe error and no data/revision change. `migration_failure_is_transactional_and_populated_upgrades_fail_closed` checks failed initial DDL rollback, checksum mismatch, and pending populated-upgrade refusal. |

Additional safety: stale generation and foreign scope writes are rejected; a pending lifecycle operation blocks snapshot writes. Independently opened adapters race publication: exactly one candidate may commit, the other retains staging and cannot mix published facts, coverage, checkpoint, revision, watermark, or success time.

## Review boundaries and open release work

- The schema includes lifecycle, alert, and managed-backup metadata boundaries only. External deletion/credential fences, native stores, startup recovery, managed backup/restore, retention, provider pagination/settlement/completeness, and analytics are not implemented.
- Only a fresh schema and validated reopen are supported. Pending migrations against a populated store fail closed before application migrations; no unsafe live database copy is used as a backup substitute.
- Within a candidate, the last accepted cursor-ordered page wins duplicate identity. Stale page replay fails cursor CAS. Source revisions are opaque, not sorted; provider-specific revision ordering must be resolved before authoritative completion is attested.
- Cross-partition event moves and overlapping authority/grain publication fail closed. They require later reconciliation rather than silently duplicating consumption.
- Full release criteria A09/A16/A19/A22/A23 remain Pending. Synthetic fixture success is a prototype contribution, not provider/native runtime evidence. No desktop GUI interaction, benchmark, production-data query, or managed recovery claim is made.
