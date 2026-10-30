# P2-05 validation evidence

All evidence below was produced on branch `jondong/p2-05-credential-rotation` in this
worktree. Every test runs against fake transports and in-memory/session secret stores;
no provider inference endpoint is ever called, and no real credential exists in the
repository, prompts, or logs.

Commands (run from the worktree root, Rust toolchain per `rust-toolchain.toml`):

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — 0 errors.
- `cargo test --locked --workspace` — 295 passed, 0 failed
  (`grep -E "test result:" /tmp/p2-05-test.log` aggregated; 295 `ok` lines).

## PRD criterion 1 — rotation keeps identity when scope matches; replacement requires explicit user action

Existing Phase 1 evidence still passes unchanged:

- `same_identity_rotation_preserves_history`
  (`apps/desktop/src-tauri/src/connections_tests.rs`): a same-identity rotation succeeds,
  keeps the scope key, retains the published snapshot, and leaves the connection Active.
- `different_identity_rotation_is_refused_without_merging`
  (`apps/desktop/src-tauri/src/connections_tests.rs`): a different identity returns
  `OperationState::Failed` + `NextAction::ReplaceConnection` (Conflict) and touches
  nothing.

New P2-05 evidence:

- `explicit_identity_replacement_never_rebinds_history`
  (`apps/desktop/src-tauri/src/connections_tests.rs`): covers the whole explicit
  replacement path. A different identity is refused with `NextAction::ReplaceConnection`;
  while the old connection is Active a second connection for the same provider is refused
  with `Conflict` (no silent replacement path); after the explicit user action (disable
  old, connect replacement) the two connections carry distinct scope keys equal to
  `crate::identity::synthetic_identity` of their own secrets, distinct readable credential
  references (`SECRET_A` vs `SECRET_B`), and the replacement publishes history under its
  own scope only. Removing the old account deletes only its artifacts — the replacement
  keeps its history and its snapshot, and the old scope's snapshot reads back `None`.

## PRD criterion 2 — mid-flight failures: no plaintext on disk, no half-applied state without a visible recovery path

Forward direction (between secret-store write and DB commit):

- `rotation_store_failure_before_commit_recovers_across_restart`
  (`apps/desktop/src-tauri/src/connections_tests.rs`): with the new test-only
  `set_fail_reference_writes` + `set_fail_reference_deletes` injections
  (`apps/desktop/src-tauri/src/connections.rs`, `RoutingSecrets`), a rotation fails after
  the journal recorded its intent but before the store committed the candidate. The
  command reports `StorageError` (retryable); the previous credential is still the live
  reference and still readable; the connection stays Active; the interrupted rotation
  stays visible as a pending `ReplaceCredentials` operation; and
  `assert_no_secret_on_disk` (new helper, recursive scan of every file under the fixture
  temp dir) proves no `SECRET_A`/`SECRET_B` bytes reached the disk. After `restart` the
  built-in recovery finishes the owed compensation (no `ReplaceCredentials` records
  remain), the session-only connection reports `NeedsCredentials`, and the explicit
  retry rotation succeeds, activates a new reference, and leaves nothing on disk.

Reverse direction (after DB commit):

- `rotation_cleanup_failure_keeps_the_activated_credential`
  (existing Phase 1 test, still green): the candidate commits, the superseded-reference
  cleanup fails; the activated credential stays live, the operation stays pending behind
  the fence, and `coordinator.recover()` converges.
- `failed_cleanup_survives_restart_and_retries` (existing Phase 1 test, still green):
  the same for connection removal across a restart.

Infrastructure-level fencing evidence (existing, still green):

- `durability_failure_before_the_journal_write_stops_the_mutation`,
  `locked_secret_store_compensates_and_never_acknowledges`,
  `failed_cleanup_after_commit_stays_pending_and_recovery_converges`
  (`crates/infrastructure/src/lifecycle/tests.rs`).

## PRD criterion 3 — tests do not call provider inference endpoints

- All router tests run against `FakeTransport` (`crates/infrastructure/src/refresh/router/tests.rs`),
  which answers from a scripted queue and reports `Offline` when exhausted; the SQLite
  repository, session secrets, and manual clock are all local. The two new P2-05 tests:
  - `rotated_credential_reaches_the_next_refresh_without_extra_calls`: a live Kimi
    connection refreshes once, a same-scope rotation runs through the real
    `lifecycle::Coordinator` (stage → journal → activate → cleanup), the call count stays
    at 1 (rotation itself issues no provider calls), the next refresh presents the rotated
    credential — asserted by `BearerToken` equality, whose `Debug` stays `[redacted]` —
    and the superseded reference is unreadable afterwards.
  - The refreshed credential path reuses `demo_connection_uses_simulated_adapter_and_never_calls_transports`
    and `missing_credential_fails_permission_required_without_network` (existing, still green).
- Response/retry limits without inference traffic: transports cap accepted bodies at
  256 KiB (`MAX_RESPONSE_BODY_BYTES`, `crates/providers/src/openrouter/transport.rs`), the
  refresh coordinator enforces `MAX_CONCURRENCY = 2`, `MAX_HISTORY_PAGES = 100`,
  `MAX_HISTORY_SECONDS = 30` (`crates/infrastructure/src/refresh.rs`) and persists
  rate-limit cooldowns instead of retry loops
  (`rate_limit_persists_cooldown_and_blocks_readmission_without_network`, still green).

## Response/retry limits without inference traffic

Covered by the FakeTransport evidence above: every limit test answers locally, cooldown
state persists through the durable store, and the new rotation test asserts zero extra
provider calls across a rotation.

## Non-goals honored

- No push, PR, merge, or archive happened from this worktree (local-first rule).
- `apps/desktop` demo mode untouched; no `prod/` paths exist in this repository.
- No real credentials, cookies, or identifying account data appear in any file added by
  this task; `SECRET_A`/`SECRET_B`/`sk-test-kimi-*` are synthetic test literals.
