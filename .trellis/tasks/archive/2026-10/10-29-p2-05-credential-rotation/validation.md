# P2-05 validation evidence

All evidence below was produced on branch `jondong/p2-05-credential-rotation` in this
worktree. Every test runs against fake transports and in-memory/session secret stores;
no provider inference endpoint is ever called, and no real credential exists in the
repository, prompts, or logs.

Commands (run from the worktree root, Rust toolchain per `rust-toolchain.toml`):

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — 0 errors.
- `cargo test --locked --workspace` — 297 passed, 0 failed
  (13 `test result: ok` lines, every one `0 failed`).
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
  keeps its history and its snapshot, the old scope's snapshot reads back `None`,
  and the removed account's credential reference no longer reads back while the
  replacement's still does.

## PRD criterion 2 — mid-flight failures: no plaintext on disk, no half-applied state without a visible recovery path

Forward direction (after the secret-store write, before the DB commit). The refusal
is produced by planting an incomplete operation projection on the connection, which
the activation transaction's mutation fence rejects inside `activate_reference` —
exactly the window between a committed candidate and a refused database commit; no
production code was changed for the injection:

- `activation_refusal_after_the_candidate_write_compensates_the_orphan`
  (`crates/infrastructure/src/lifecycle/tests.rs`): after the fence refuses the
  activation, the candidate had already been written to the store; the inline
  compensation deletes the **orphan candidate — never the previous reference** —
  the previous reference keeps serving, the journal is clean, the planted fence
  stays up until recovery finishes it, and a retry with a fresh candidate succeeds.
- `interrupted_activation_owed_cleanup_survives_to_recovery_and_keeps_the_previous`
  (`crates/infrastructure/src/lifecycle/tests.rs`): the same refusal with the
  compensating delete also failing — the candidate is present in the store, never
  activated, and the interruption stays visible as `CompensationPending` behind the
  fence while the previous reference keeps serving. Recovery with a healthy store
  deletes the orphan candidate (never the previous reference) and finishes both
  interrupted operations.
- `rotation_activation_failure_after_the_store_write_recovers_across_restart`
  (`apps/desktop/src-tauri/src/connections_tests.rs`): the same window through the
  IPC command, with `set_fail_reference_deletes` (the remaining test-only
  `RoutingSecrets` injection) keeping the inline compensation from running. The
  command reports `StorageError`; the interrupted record names its orphan candidate,
  which **reads back from the store** (proof the store write succeeded before the
  database refusal); the previous credential is still the live, readable reference;
  the connection stays Active; and `assert_no_secret_on_disk` (recursive scan of
  every file under the fixture temp dir) proves no `SECRET_A`/`SECRET_B` bytes
  reached the disk. After `restart` the built-in recovery finishes both interrupted
  operations (no `ReplaceCredentials` records remain), the session-only connection
  reports `NeedsCredentials`, and the explicit retry rotation succeeds, activates a
  new reference, and leaves nothing on disk. Session-only caveat: the store is
  in-memory, so post-restart store-content assertions are carried by the two
  infrastructure tests above.

Honest IPC note (review finding): the `replace_credentials` envelope does not
return the interrupted operation's id, and no IPC command lists lifecycle
operations, so a renderer cannot poll that specific operation. What the failure
does guarantee at the IPC surface: a typed `StorageError` envelope, an untouched
working credential, and — because the journal/projection are durable — the
automatic recovery that `restart`-time `Coordinator::recover()` performs; the
pending record is visible to any component with repository access (as asserted
in the tests). Returning operation ids over IPC is left as follow-up work.
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
