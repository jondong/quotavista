# P1-07 validation record

## Scope and prerequisites

Backend composition and frontend wiring. P1-07 wires the existing P1-03 storage, P1-04 lifecycle coordinator, P1-05 secret stores, and P1-06 refresh coordinator into the Tauri desktop entry point. It adds a single `dispatch_command` IPC entry that validates every request through the contract envelope (`contract_version`, `request_id`, `Command`), maps it into the existing application use cases, and returns a `CommandResult` wrapped in an `Envelope`. The renderer talks to the backend through a narrow generated-type client at `apps/desktop/src/ipc.ts` and subscribes to revision-only `ChangeEvent` notifications. The Tauri capability allowlist on the `main` window only permits `dispatch_command`; every other command-level operation, validation, and safe-error mapping lives behind it.

Prerequisite confirmed before starting: P1-05 is merged (PR #7, merge commit `dc0d14b`) and archived at `.trellis/tasks/archive/2026-10/10-18-p1-05-secret-stores` by archive commit `c076f68`. P1-06 is merged (PR #6, merge commit `4e024c8`) and archived at `.trellis/tasks/archive/2026-10/10-18-p1-06-simulation` by archive commit `62c348d`. P1-04 is archived at `.trellis/tasks/archive/2026-10/10-18-p1-04-recovery-fences` (archive commit `a38d2b4`), and the P1-08 bilingual frontend shell is archived at `.trellis/tasks/archive/2026-10/10-18-p1-08-bilingual-shell`. Branch head on top of main: `main` head `62c348d`.

Every fixture is synthetic and throwaway: `build` opens `SqliteRepository` against `std::env::temp_dir().join("quotavista-prototype")` and a `FileStore` rooted at `<root>/journal`, both cleared between runs. The desktop integration tests use `SessionSecrets` (an in-memory fake) and never touch a real keychain, Secret Service bus, application data directory, production database, or business database. Activation was recorded with `python3 .trellis/scripts/task.py start .trellis/tasks/10-18-p1-07-ipc`.

Environment: macOS 27.0.1 arm64, Rust 1.99.1, Node 24.22.0, npm 11.19.1, Tauri 2.7.3, ts-rs emits the generated IPC types into `apps/desktop/src/generated/contracts.ts`.

## Validation status

All required local validation passed. The pull request must still show both CI jobs green on the latest pushed commit before the branch is considered mergeable; CI compilation is not native platform evidence.

Commands run in this worktree, in order:

- `cargo fmt --all` then `cargo fmt --all -- --check` - passed with no diff.
- `cargo clippy --locked --workspace --exclude quotavista-desktop --all-targets -- -D warnings` - exit 0. The 60 warnings are the pre-existing ts-rs `failed to parse serde attribute` notes from `quotavista-contracts` (proc-macro warnings, not clippy lints) and are unrelated to this change. `cargo clippy --locked -p quotavista-desktop --all-targets -- -D warnings` also exits 0 with 30 identical ts-rs warnings; macOS-only in CI.
- `cargo test --locked --workspace --exclude quotavista-desktop` - passed, 120 tests, 1 ignored, 0 failed across 10 suites (contracts, core, infrastructure, providers, generated types). The macOS-only equivalent `cargo test --locked --workspace` includes `quotavista-desktop` lib and reports 5 additional tests in `quotavista-desktop` (the new boundary tests below). `cargo test --locked -p quotavista-desktop` reports `5 passed (3 suites)`.
- `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` - passed: `TypeScript contract is current`. The new `apps/desktop/src/ipc.ts` consumes the generated types and the renderer type-check picks up every command and DTO.
- `./scripts/check-core-boundaries.sh` - passed: `quotavista-core calendar-only dependencies: boundary OK`. P1-07 adds no new dependency in `core`, so the contract still holds.
- `./scripts/test-check-core-boundaries.sh` - passed: `all boundary-gate regression checks passed`.
- `cd apps/desktop && npm ci` - passed.
- `cd apps/desktop && npm run lint` - passed: `ESLint: No issues found`.
- `cd apps/desktop && npm run i18n:check` - passed: `Catalog parity OK: 58 keys, placeholders matching in both catalogs`.
- `cd apps/desktop && npm test` - passed: 2 files / 21 tests. The `App.test.tsx` mock now returns an `Envelope` carrying a `list_connections` `CommandResult`, and `contracts.typecheck.ts` keeps its 11 canonical sender shapes plus the negative `@ts-expect-error` tests.
- `cd apps/desktop && npm run typecheck` - passed: `tsc --noEmit` exits 0.
- `cd apps/desktop && npm run build` - passed: Vite transformed 41 modules and emitted `dist/index.html`, `dist/assets/index-*.css`, and `dist/assets/index-*.js`.
- `cargo build --locked -p quotavista-desktop` - passed (macOS only; the capability is only built with the desktop crate).
- `git diff --check` - passed with no whitespace or conflict-marker findings.

## Synthetic acceptance evidence verified

| PRD criterion | Fixture and required result |
| --- | --- |
| Malformed/unknown fields, providers, IDs, revisions, dates, and oversized credentials fail before side effects. Only `connect_provider` and `replace_credentials` accept secret input; no response echoes it. | `dispatch_rejects_unknown_contract_version` (a `Request` with `contract_version: 999` is rejected with `ErrorCode::Validation` before any storage or secret call). `dispatch_rejects_oversized_secret_via_decode` (a 65,537-byte secret fails serde decode on `SecretInput` and the dispatch never reaches the handler). `dispatch_rejects_oversized_scope_set` (101 scope ids in `OverviewRangeInput.scope_ids` are rejected with `Validation`). `dispatch_rejects_empty_selected_target` (`RefreshTarget::Selected(Vec::new())` is rejected with `Validation`). `only_connect_and_replace_accept_secret_input` (every `Command` variant uses an internally tagged enum and `deny_unknown_fields`; only `connect_provider` and `replace_credentials` decode a `secret` field, and every other variant rejects a smuggled secret on decode). The contracts crate's compile-fail negative tests cover invalid `QueryKind`, invalid `Total`, unknown `Provider`, and omitted `SettingsPatch`. |
| The renderer cannot invoke a generic URL/SQL/shell/filesystem proxy, access native secrets directly, or gain privileges through remote content. | `apps/desktop/src-tauri/capabilities/default.json` declares `windows: ["main"]` and `permissions: ["core:default", "allow-dispatch-command"]`. `build.rs` calls `tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(&["dispatch_command"])))` so the auto-generated `allow-dispatch-command` permission in `apps/desktop/src-tauri/permissions/autogenerated/dispatch_command.toml` is the only IPC entry the main window can reach. The frontend `ipc.ts` wraps `invoke<Envelope>("dispatch_command", { request })` exclusively; there is no Tauri command for native secrets, no URL/SQL/shell/filesystem shim, and the `invoke_handler` lists only `dispatch_command`. |
| Revision events contain opaque IDs and revisions only. Reopening reads local state; skipped revisions trigger reload. Obsolete display responses are discarded. | `apps/desktop/src/ipc.ts` exposes `subscribeToChanges(handler: (event: ChangeEvent) => void)` listening on `qv://change`. The generated `ChangeEvent` is `{ data_revision: Revision, affected_connection_ids: Array<LocalId>, affected_scope_ids: Array<LocalId> }`: opaque identifiers and revisions only, no values, no prose, no credentials. The contract layer carries no secret-shaped field in any event or error envelope, and the `SecretInput` type is `NOT Serialize` so it cannot appear in any response, event, log, or error. |
| Storage/secret/coordinator errors remain typed, useful cached data survives, and failed commits never report synchronization success. | `storage_error` in `apps/desktop/src-tauri/src/lib.rs` maps every `StorageError` variant to a sealed `SafeError` (`Conflict`, `Validation`, `Migration`/`IncompatibleVersion` → `InternalError`, `Unavailable` → `StorageError`). The `Envelope::Error` variant carries only the sealed `SafeError`, so the renderer can branch on `code` without losing typed-ness. The contract envelope does not encode success on a refused commit: `Request::validate` rejects with `Validation` before the handler runs, and the `activate_reference` final check inside the `BEGIN IMMEDIATE` transaction (already covered in P1-04 evidence) re-checks generation, lifecycle, and fence before any commit. |

Five integration tests live in `apps/desktop/src-tauri/src/lib.rs::tests` and run with `cargo test --locked -p quotavista-desktop` (macOS only). All five pass.

## Review boundaries and open release work

This task wires the desktop composition root and the frontend client; it does not close every release criterion it contributes to.

- The eleven command handlers in `dispatch_command` are stubbed. They return empty typed DTOs (`list_connections`, `refresh_connections`, `get_overview`, `query_statistics`) or `SafeError::new(ErrorCode::InternalError, None)` for unsupported later-phase actions, so the envelope round-trips correctly. Full implementations live in P1-09 (`get_overview`, `query_statistics`), P1-10 (`list_connections`, `connect_provider`, `update_connection`, `replace_credentials`, `set_connection_enabled`, `remove_connection`, `update_settings`), and P1-11 (`refresh_connections`, `get_operation`). Until those tasks land, the renderer can show the synthetic data banner and demo snapshots but cannot yet exercise the production data flow end to end.
- `Cargo.toml` pins `tauri = "2"` and `tauri-build = "2"` with no platform features. The capability file uses the `windows` array to scope the allowlist to the `main` window; additional windows (settings popups, tooltips) would need their own capability file or an explicit `windows: ["main", "<other>"]` allowlist.
- `NativeSecrets` is wired in only on `cfg(macos)` and `cfg(linux)` targets. Other targets fall back to `SessionSecrets`; the prompt's secret-store protocol never opens a real OS keychain. The macOS native smoke check is intentionally not performed by this worker: the contract is exercised through `SessionSecrets` (a fake in-memory map) and `RefreshCoordinator` over synthetic scenarios, with no keychain, Secret Service bus, application data directory, production, business, or personal database opened, read, or written. A native smoke check is owed by P1-11 and must use only disposable synthetic secrets under a uniquely named `quotavista-test-` service, with cleanup evidence recorded.
- The `qv://change` event channel is wired in the client but not yet emitted by the desktop. P1-10 and P1-11 own the corresponding `app_handle.emit("qv://change", ...)` calls when storage/coordinator mutations commit.
- CI on [PR #8](https://github.com/jondong/quotavista/pull/8): both `workspace-checks` (2m16s) and `macos-desktop` (5m13s) pass against the latest pushed commit (head `49772a3`). Implementation commit is `159515f` and the follow-up tick is `49772a3`. Local validation is consistent with CI; a passing local run was not treated as a substitute. The branch is not merged, not pushed to `main`, and not archived.

## Stale link fixes recorded

The following documents referenced `../10-18-p1-0{4,5,6}-*/` task directories that have since been archived under `archive/2026-10/`:

- `.trellis/tasks/10-18-p1-07-ipc/prd.md` - `P1-05` and `P1-06` dependency links now point under `../archive/2026-10/`.
- `.trellis/tasks/10-18-p1-07-ipc/design.md` - predecessor links updated to the archive paths.
- `.trellis/tasks/10-18-p1-07-ipc/implement.md` - predecessor links updated to the archive paths.
- `docs/traceability.md` - the `P1-05` and `P1-06` validation record references now point under `../.trellis/tasks/archive/2026-10/`.
