# P1-04 validation record

## Scope and prerequisites

Backend scope only. P1-04 adds the nonsecret lifecycle model in `quotavista_core::journal`, the durable external store in `quotavista_infrastructure::durable`, and the recovery coordinator in `quotavista_infrastructure::lifecycle`, plus the `lifecycle_operations` projection methods on the existing P1-03 SQLite adapter. No contracts, generated TypeScript, frontend, desktop command, migration, or provider change is included, and `STORE_KIND` stays synthetic because the desktop composition task wires these pieces together.

Prerequisite confirmed before starting: P1-03 is merged (`gh pr view 4` reports `state: MERGED`, merge commit `70fedfa`) and archived at `.trellis/tasks/archive/2026-10/10-18-p1-03-sqlite` by archive commit `1f25874`, which is also where this branch starts from `main`.

Every fixture is synthetic and throwaway: temporary directories from `tempfile::TempDir` hold the SQLite database, the journal, and the ledger; the secret-reference store is an in-memory fake map keyed by synthetic reference identifiers. No real application data directory was resolved, no OS keychain or native secret store was opened, and no production, business, or personal database was connected, not even for reads. No credential value or environment secret was written into any file, fixture, or artifact.

Activation was recorded with `python3 .trellis/scripts/task.py start .trellis/tasks/10-18-p1-04-recovery-fences` after reading its `--help` output. The script reported degraded mode because no session identity is available in this worker, so the active-task pointer stayed in gitignored `.trellis/.runtime/` and only `task.json` changed.

Environment: macOS 27.0.1 arm64, Rust 1.99.1, Node 24.22.0, npm 11.19.1, SQLx 0.9.0 with bundled SQLite 3.51.3 asserted through `SELECT sqlite_version()` rather than a system CLI.

## Validation status

All required local validation passed. The pull request must still show both CI jobs green on the latest pushed commit before the branch is considered mergeable; CI compilation is not native platform evidence.

Commands run in this worktree, in order:

- `cargo fmt --all` then `cargo fmt --all -- --check` - passed with no diff.
- `./scripts/check-core-boundaries.sh` - passed: `quotavista-core` keeps calendar-only dependencies, so the new `journal` module adds no crate dependency and no serialization type.
- `./scripts/test-check-core-boundaries.sh` - passed: `all boundary-gate regression checks passed`.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` - passed with zero errors. The only warnings emitted are the pre-existing ts-rs `failed to parse serde attribute` notes from `quotavista-contracts`, unrelated to this change. One clippy failure was fixed during the run (`clippy::unnecessary_mut_passed` on `owe_cleanup`).
- `cargo test --locked --workspace` - passed, 70 tests, 0 failed (57 unit/integration plus 13 contracts doc-tests): `quotavista-infrastructure` lib 29 (11 SQLite adapter fixtures, 3 durable-store fixtures, 14 lifecycle recovery fixtures, 1 scaffold test), `quotavista-contracts` integration 16, `quotavista-core` lib 10, `quotavista-desktop` lib 1, `quotavista-providers` 1. The Linux CI variant `cargo test --locked --workspace --exclude quotavista-desktop` passed 69, and `cargo clippy --locked --workspace --exclude quotavista-desktop --all-targets -- -D warnings` was clean, so both CI jobs have a local equivalent. `cargo build --locked -p quotavista-desktop` also finished clean.
- `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` - passed: `TypeScript contract is current`. No DTO changed, so the generated contract and the desktop type-check input are untouched.
- `cd apps/desktop && npm ci && npm run lint && npm run i18n:check && npm test && npm run typecheck && npm run build` - passed: 239 packages installed, ESLint clean, `Catalog parity OK: 58 keys, placeholders matching in both catalogs`, Vitest 2 files / 21 tests passed, `tsc --noEmit` clean, Vite build transformed 39 modules. This worktree ships no frontend change; the commands run because CI runs them on every pull request.
- `git diff --check` - passed with no whitespace or conflict-marker findings.

Final validation commands and results: `cargo fmt --all -- --check` clean, `cargo clippy --locked --workspace --all-targets -- -D warnings` clean, `cargo test --locked --workspace` 70 passed / 0 failed, both boundary scripts clean, contract generation check clean, desktop checks clean, `git diff --check` clean.

- PR CI: [PR #5](https://github.com/jondong/quotavista/pull/5) targets `main` from this task branch. Implementation commit: `5f7fadc`. The latest-commit check results and worker handoff are recorded on the PR after verification; a passing local run is not a substitute. This task remains in review and is not archived.

## Synthetic acceptance evidence verified

| PRD criterion | Fixture and required result |
| --- | --- |
| Injected interruptions before and after the journal write, candidate activation, and cleanup converge idempotently or stay visibly pending; corrupt journals block unsafe recovery | `durability_failure_before_the_journal_write_stops_the_mutation` (`FakeDurable::fail_after(0)` refuses the first replace, so no secret is written and no projection is created); `failed_cleanup_after_commit_stays_pending_and_recovery_converges` and `recovery_removes_an_orphan_candidate_the_projection_never_recorded` (a fresh coordinator over the same throwaway directory converges, and re-running recovery is a no-op); `corrupt_external_state_blocks_recovery_instead_of_assuming_clean` (a corrupt journal, a corrupt ledger, and a truncated ledger each make `Coordinator::open` fail, and absent state on a fresh install opens empty without inferring finished cleanup) |
| An obsolete generation cannot commit while disable, delete, or rotation runs, with the final check inside the write transaction under the mutation fence | `obsolete_generation_cannot_activate_and_the_candidate_is_compensated` (`activate_reference` fails with `StorageError::Conflict` after a concurrent `set_lifecycle` bumped the generation, the candidate reference is deleted, and the connection keeps its previous state); `pending_projection_fences_result_writes_below_the_coordinator` (an incomplete `lifecycle_operations` row makes `save_snapshot` fail even when the caller holds a current generation) |
| The deletion ledger prevents dispatch and rejects restoration of a manifest containing deleted opaque identifiers; failure does not acknowledge deletion | `deletion_ledger_fences_dispatch_and_rejects_a_restoring_manifest` (a raw-SQL restore of an earlier backup state is simulated, then `schedule_context` and `admit_restore` both refuse because the ledger survived the restore); `failed_deletion_cleanup_stays_visible_retryable_and_blocks_restore` (a refused reference deletion leaves the ledger entry `pending_cleanup`, keeps the operation visible, and `admit_restore` returns `RecoveryRequired`); `deletion_stops_before_any_secret_write_when_durability_fails` (durability failure during the ledger write stops before any secret call) |
| Journal files carry no credentials, provider identity, or usage data, and failure to establish durability stops the mutation before a secret write | `external_state_carries_opaque_identifiers_only` (asserts the raw journal and ledger text contain no synthetic secret value, provider identifier, alias, scope key, or usage metric, while still containing the opaque operation id, kind, and phase); `durability_failure_before_the_journal_write_stops_the_mutation` and `deletion_stops_before_any_secret_write_when_durability_fails` (the fake secret store records no write); `durable::tests::replacement_is_atomic_and_previous_contents_survive_a_failed_rename` and `durable::tests::durability_failures_stop_the_write_and_reads_never_infer_absence` (injected temp-write, sync, and rename failures leave the previous contents intact and an unreadable entry reports `RecoveryRequired` instead of absence); `durable::tests::entry_names_cannot_leave_the_root_directory` |

The 14 lifecycle fixtures and 3 durable-store fixtures also cover the boundaries the PRD names but does not enumerate individually: setup activation from `pending_setup`, rotation with an active previous reference, compensation when the secret store is locked (`locked_secret_store_compensates_and_never_acknowledges`), recovery isolation so one locked connection does not stop the others (`one_locked_connection_does_not_stop_recovery_for_the_others`), and the recovery-before-mutation gate (`recovery_runs_before_any_mutation_or_scheduling`).

## Review boundaries and open release work

This task establishes recovery fences; it does not complete the release criteria it contributes to.

- No real provider identity verification, no credential UI, and no native secret store. `SecretReferenceStore` is a port with an in-memory fake; P1-05 supplies the platform implementation.
- No managed backup creation, purge, or restore execution. `BackupManifest` validation and `admit_restore` decide admission only; the SQLite consistent-backup API, backup retention, and purge are later-phase work, so a deletion today cannot yet prove that every backup copy was removed.
- Local fact and configuration purge during deletion is not implemented, which is why `LedgerState::CleanupComplete` means "every cleanup step this build owns finished" and why ledger compaction is deliberately not automatic.
- The coordinator is not composed into desktop startup or the IPC command set, so no user-visible lifecycle flow runs through it yet; P1-07 and P1-11 do the composition.
- Durability is asserted as a call sequence against injected failures. File `sync_all` and directory sync are advisory on network shares, removable media, and Windows directory handles, and Phase 1 does not measure real media behaviour.
- Platform gaps: no Linux or Windows native run was performed by this worker; CI provides Linux compilation and test evidence, and the macOS CI job covers the full workspace. Neither is native platform evidence for durability on user hardware.
- Release acceptance for A18, A20, A21, and A23 therefore remains Pending in `docs/traceability.md`.
