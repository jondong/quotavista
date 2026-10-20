# P1-05 validation record

## Scope and prerequisites

- Implements the PRD in [prd.md](prd.md): a `SecretStore` port with pinned keyring-core native adapters (macOS Keychain, Linux Secret Service), journal-backed candidate-write/activate/cleanup, explicit session-only storage, and distinct locked/unavailable/needs-credentials states.
- Predecessor confirmation: P1-04 is merged (PR #5, merge commit `46f8db9`) and archived under `.trellis/tasks/archive/2026-10/10-18-p1-04-recovery-fences` (archive commit `a38d2b4`).
- Fixture hygiene: all tests use fake stores, temporary directories, and throwaway SQLite databases with synthetic fixtures. The macOS native smoke check used disposable synthetic secrets under the uniquely named test-only service `quotavista-test-p1-05-<pid>`; no other keychain entry was read, modified, or deleted; the test entries were removed afterwards. No real credentials, application data directories, or production databases were touched.
- Environment: mac-mini, macOS 27.0.1, Rust 1.99.1.
- Linux native evidence: **pending** — this host has no Linux Secret Service session bus; the `#[ignore]`d `linux_secret_service_synthetic_roundtrip` check records itself as pending instead of failing when the store is unavailable.

## Validation status

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | pass, 0 errors |
| `cargo test --locked --workspace` | pass, 91 passed / 0 failed / 1 ignored (13 suites) |
| `./scripts/check-core-boundaries.sh` | pass (`quotavista-core calendar-only dependencies: boundary OK`) |
| `./scripts/test-check-core-boundaries.sh` | pass (all boundary-gate regression checks) |
| `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` | pass (`TypeScript contract is current`) |
| `npm ci && npm run lint && npm run i18n:check && npm test && npm run typecheck && npm run build` (apps/desktop) | pass (ESLint clean, catalog parity OK, 21/21 tests, tsc clean, vite build OK) |
| `cargo test --locked -p quotavista-infrastructure macos_keychain_synthetic_roundtrip -- --ignored --nocapture` | pass (native smoke check, see evidence below) |
| `git diff --check` | pass |
| PR CI | pass (workspace-checks + macos-desktop, run 39387091597; first run 39386430090 caught a missing `crypto-rust` feature for the Linux adapter, fixed in 4f16e5b). Round 1 fixes re-validated green in run 39392892679 (workspace-checks 1m56s, macos-desktop 2m40s, head a399180) |

### Native smoke-check evidence (macOS Keychain)

- Host: mac-mini (macOS 27.0.1).
- Service: `quotavista-test-p1-05-52754` (test-only, uniquely named per run).
- Result: `test secrets::tests::macos_keychain_synthetic_roundtrip ... ok`.
- Cleanup evidence (test output): `qv1:macos-keychain:synthetic-entry-a` and `qv1:macos-keychain:synthetic-entry-b` removed from the test service; both read back as `NeedsCredentials`. Deletion of an absent entry also succeeds (recovery idempotency).

## Synthetic acceptance evidence verified

| PRD criterion | Fixture and required result |
| --- | --- |
| Synthetic create/read/rotate/delete on each available native store; rotation failure keeps the previous active credential and exposes failed compensation | `secrets::tests::session_only_credentials_flow_through_the_journal_backed_lifecycle` (setup, rotation with superseded cleanup, deletion through the journal-backed lifecycle); `secrets::tests::failed_rotation_keeps_previous_and_failed_compensation_stays_visible` (locked candidate write keeps the previous credential active; a failed cleanup of the superseded reference stays pending in the journal across restart until recovery completes it); `secrets::tests::macos_keychain_synthetic_roundtrip` (native macOS Keychain roundtrip with the previous entry preserved until the candidate commits). Linux native roundtrip: pending (headless host, see above). |
| Locked and unavailable store errors remain distinct; no file or plaintext persistence fallback | `secrets::tests::locked_and_unavailable_are_distinct_and_leave_the_connection_untouched` (`Locked` and `Unavailable` surface as distinct errors and leave the connection active and untouched); `lifecycle::tests` locked/unavailable recovery cases; the stores have no plaintext persistence path at all. |
| Explicit session-only choice works until exit; restart retains nonsecret data, has no retrievable session secret, enters needs-credentials | `secrets::tests::session_only_restart_loses_the_secret_and_enters_needs_credentials` (journals and SQLite survive, process memory does not; the connection's alias and reference are retained, the secret reads back `NeedsCredentials`, and reconciliation transitions the connection to `NeedsCredentials`); `core::secrets` unit tests for staged-commit semantics. |
| Representative success/error/serialization paths do not leak the synthetic secret into SQLite, journals, logs, events, or DTOs; Linux evidence pending when unavailable | `secrets::tests::secret_values_never_reach_journals_or_the_database` (secret values absent from journal, ledger, and all SQLite files including WAL sidecars, while opaque references and nonsecret configuration are present — sanity assertions included); error and reference validators never echo rejected values (`core::secrets` unit tests); journal/ledger A10 fixture from P1-04 still passes. |

## Review boundaries and open release work

- The `SecretStore` port, native adapters, session-only store, and `reconcile_connection_credentials` are implemented but not yet composed into desktop startup or provider calls; P1-06 (simulated providers) and P1-07 (restricted IPC) wire them in.
- Candidate validation is synthetic in Phase 1: the secret validator enforces shape only (non-empty, size, no header line breaks); it cannot prove provider access until providers exist.
- Linux native smoke evidence remains pending until a host with a Secret Service session bus is available; the check is in place and non-destructive.

## Review round 1 (PR #7, head f080ade)

Reviewer requested changes; all were addressed on this branch:

1. **Blocking — native delete left staged values committable.** `Command::Delete` now removes the staged entry from the worker before calling the backend, so a later `Write` of the same reference fails without touching the backend; real backend deletion errors are still reported, so durable cleanup stays retryable. Evidence: `delete_drops_the_staged_value_and_backend_errors_stay_reported` (stage → delete → write fails with the backend never written; refused delete returns `Locked` and still drops the staged bytes) and `abort_after_failed_begin_operation_drops_the_staged_candidate` (Coordinator abort after a refused `begin_operation` leaves the candidate uncommittable and releases the fence).
2. **Should-fix — sync store port called inline on the async runtime.** `Coordinator` routes `write_candidate`/`delete_reference` through `tokio::task::spawn_blocking` (`store_call`); `reconcile_connection_credentials` wraps its store read the same way. A panicking or dead worker surfaces as `Unavailable`, never a hang on the async worker.
3. **Should-fix — stale PRD planning status.** `prd.md` now points to this validation record.
4. **Nit — backend spec mention.** `.trellis/spec/backend/index.md` names the P1-05 secret-store boundary and the blocking-call offload rule. Linux smoke evidence stays honestly pending above.

Re-validated after the fixes: `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` (93 passed / 0 failed / 1 ignored), `./scripts/check-core-boundaries.sh`, `./scripts/test-check-core-boundaries.sh`, `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check`, `git diff --check`, and the ignored macOS Keychain smoke check (passed, entries cleaned up under `quotavista-test-`).
