# P1-10 validation record

## Scope and prerequisites

Connection and settings flows. P1-10 implements the six previously deferred mutation and settings commands (`connect_provider`, `update_connection`, `replace_credentials`, `set_connection_enabled`, `remove_connection`, `update_settings`) behind the existing P1-07 single-command IPC boundary, and replaces the P1-08 demo Connections/Settings screens with real IPC-driven views. Connection operations are modelled as backend operation state with safe progress and retry eligibility; the UI reflects operation status and never reports optimistic success. Settings (refresh interval including Off, reporting zone, UI locale, display currency, quota/balance thresholds) persist through the versioned backend settings row with optimistic revision checks.

Prerequisite confirmed before starting: P1-07 is merged (PR #8, head `9188637`, merge commit `34fc4c3`) and archived at `.trellis/tasks/archive/2026-10/10-18-p1-07-ipc` by archive commit `44a0f33` (the branch base of this task). P1-08 is archived at `.trellis/tasks/archive/2026-10/10-18-p1-08-bilingual-shell` (archive commit `a54b56b`). Activation was recorded with `python3 .trellis/scripts/task.py start 10-18-p1-10-connections-settings`.

Environment: macOS 27.0.1 arm64, Rust 1.99.1, Node 24.22.0, npm 11.19.1, Tauri 2.12.4, ts-rs generates `apps/desktop/src/generated/contracts.ts`.

Every fixture is synthetic and throwaway: the desktop integration tests build the application over `tempfile::TempDir` roots with `SecretChoice::SessionOnly` (the in-memory fake) and the simulated providers; no real keychain entry, Secret Service bus, application data directory, production database, or business database was opened, read, or written. The renderer tests run in jsdom against mocked `invoke`/`listen`; no native window was driven.

## Design decisions recorded during implementation

- **Settings read path.** The reviewed eleven-command contract has no `get_settings` command. Reading settings uses `update_settings` with a canonical empty patch (all six fields `null`), which returns the current `SettingsDto` without touching the revision; a non-empty patch keeps the optimistic `expected_settings_revision` compare-and-swap and answers a stale revision with `conflict`. Omission and `null` both mean "leave unchanged", matching the frontend contract guidance.
- **Thresholds.** The PRD deliverable "threshold preferences" added `quota_threshold` (`Ratio`, default `0.2`) and `balance_threshold` (`Decimal`, `0` = none) to the core `Settings`, the settings row (migration `0002_settings_thresholds.sql`, additive columns with defaults), `SettingsDto` (required), and `SettingsPatch` (optional). Evaluation of alerts stays later-phase; the values are stored, round-tripped, and surfaced only.
- **Migration default repair.** `0002_settings_thresholds.sql` also sets `refresh_minutes = 5` on the never-saved seeded row only (`WHERE id = 1 AND revision = 0 AND refresh_minutes IS NULL`), so the requirements' 5-minute default applies to fresh databases while an explicitly saved `Off` survives.
- **Synthetic identity.** Connection identity is derived from the submitted secret itself (SHA-256 of the value, self-contained implementation in `apps/desktop/src-tauri/src/identity.rs`, no new dependencies) and stored as the provider scope key. A same-scope rotation therefore preserves history under the same scope; a different secret is a different, unverified identity that is refused with a failed operation (`next_action: replace_connection`) instead of silently merging into the existing scope.
- **One active connection per provider.** `connect_provider` refuses while an active connection for the provider exists (`conflict`), and the SQLite partial unique index `one_active_provider` (in place since P1-03) backs the pre-check against racing candidates: the loser's `create_connection` fails with `conflict` and its staged candidate reference is deleted.
- **Store choice and session-only mode.** `RoutingSecrets` (`apps/desktop/src-tauri/src/connections.rs`) routes each secret operation by the reference's backend tag across the native store (when it opened) and an explicit process-memory `SessionSecrets`. References are minted per the requested `storage_mode`; a `persistent` request in a session-only composition answers `secret_store_unavailable` rather than silently downgrading. At startup every active connection is reconciled: a secret that no longer reads (session-only after restart) moves the connection to `needs_credentials` without collecting anything.
- **Removal acknowledgement.** Removal follows the P1-04 ledger boundary: `begin_deletion` (ledger pending, lifecycle `deletion_pending`, generation bump) → delete the owed secret reference → acknowledge the ledger → delete the connection row (cascading scopes, projections, usage facts, and lifecycle operations). A cleanup failure leaves `deletion_pending` with a retryable operation that survives restart and can be retried from the UI. The `managed_backup_scopes` opaque membership survives live-row deletion by design; managed-backup purge/restore execution remains explicitly pending.
- **Commit announcement.** Every connection-affecting mutation advances the store `data_revision` (new `Repository::advance_data_revision` port method) and then emits the revision-only `ChangeEvent` through the existing `publish_change` helper, so renderer lists invalidate exactly as they do for refreshes. Settings keep their own settings revision and do not announce connection events.

## PRD criterion evidence

Criterion, method, result. All Rust tests below live in `apps/desktop/src-tauri/src/connections_tests.rs` unless another path is named; all renderer tests live under `apps/desktop/src/`.

### Criterion 1 — safe failures; secrets clear on submission and never enter browser storage

- `invalid_aliases_and_secrets_fail_safely` (Rust): empty alias, 81-code-point alias, header-invalid secret (embedded `\n`), and a 65,537-byte secret each answer `Envelope::Error` with `code: "validation"` (safe message, no echo of the rejected value) and leave no connection row, scope, or staged reference behind.
- `stale_revisions_fail_safely` (Rust): a rename with a stale `expected_config_revision` and a settings save with a stale `expected_settings_revision` both answer `code: "conflict"` without changing the stored alias or settings.
- `secrets_never_leave_the_store` (Rust): after add and rotate, the serialized `Envelope::Ok` and `Envelope::Error` payloads of every connection/settings command (connect, rename, disable, enable, rotate, stale-revision rotate, refresh, remove, list) plus the emitted `ChangeEvent` payloads are searched for both secret plaintexts; neither appears.
- `submits an add with the typed fields and clears the secret after success` (`screens/ConnectionsScreen.test.tsx`): the add form sends `connect_provider` with provider/alias/credential kind/secret/storage mode, the secret input is emptied after the response, and the `localStorage.setItem` spy is never called.
- `clears the secret after a failed submission and shows the typed error` (same file): a typed validation envelope leaves the secret input cleared and renders the localized validation message.
- `surfaces a stale-revision conflict, reloads, and never overwrites the newer choice` and `surfaces a validation failure without clobbering the drafts` (`screens/SettingsScreen.test.tsx`): conflict reloads and shows the newer stored values; validation keeps the drafts for correction.

### Criterion 2 — same-scope rotation preserves history; different identity cannot merge; one active per provider

- `same_identity_rotation_preserves_history` (Rust): a refresh commits facts, then rotation with the same secret re-uses the identical scope key (identity derived from the secret), the previously published usage snapshot is still retained under that scope (`repository.snapshot(...).is_some()`), and the new reference activates.
- `different_identity_rotation_is_refused_without_merging` (Rust): rotation with a different secret returns a `failed` operation with `next_action: "replace_connection"` and `code: "conflict"`, the connection's generation and alias are unchanged, and the connection still has exactly one scope — no silent merge, the working credential untouched.
- `racing_candidates_yield_one_active_connection_per_provider` (Rust): two `connect_provider` requests for the same provider race under `tokio::join!`; exactly one succeeds, the other answers `conflict`, and exactly one connection row of any lifecycle remains afterwards (the losing setup leaves no `pending_setup` ghost row or scope; pre-check plus the `one_active_provider` partial unique index).
- `reflects a failed rotation without optimistic success` (`screens/ConnectionsScreen.test.tsx`): a failed `replace_credentials` operation renders the operation state as failed with the replace-connection hint; the row is not optimistically updated.

### Criterion 3 — disable/remove fence late work; deletion pending survives restart; managed-backup guarantees explicitly pending

- `disabling_fences_late_work` (Rust): after `set_connection_enabled(false)`, a stale `WriteContext` (old generation) cannot mutate the connection; re-enabling restores `active` through a fresh generation.
- `removal_cleans_up_completely` (Rust): successful removal deletes the connection row and announces a `ChangeEvent` carrying the affected connection id.
- `failed_cleanup_survives_restart_and_retries` (Rust): with reference deletions failing (injected store failure), removal reports a retryable `failed` operation, the connection stays `deletion_pending`; after a restart (fresh process over the same directory) the connection is still `deletion_pending`, and retrying removal completes it.
- `renders the pending managed-backup note explicitly` (`screens/SettingsScreen.test.tsx`): the settings screen states that managed-backup deletion guarantees remain explicitly pending. The backend removal boundary likewise only acknowledges the artifacts that exist in this phase; managed-backup purge/restore stays pending and is not claimed anywhere.

### Criterion 4 — locale and refresh-Off survive restart; session-only restart returns Needs credentials; locale switch preserves form state

- `explicit_settings_survive_restart` (Rust): `ui_locale: zh-CN`, `polling: disabled` (Off), reporting zone `Asia/Shanghai`, currency `CNY`, and thresholds `0.5`/`10` persist across a restart of the same directory; the settings revision advanced exactly once.
- `session_only_connection_needs_credentials_after_restart` (Rust): after a restart, the session-only connection reports `needs_credentials` (no implicit collection), and rotating with the same secret recovers it to `active`.
- `empty_patch_reads_current_settings_without_bumping_the_revision` (Rust): the canonical empty patch returns the current settings (default polling `five_minutes`) without changing the revision.
- `applies an explicitly saved backend locale over system detection` (`App.test.tsx`): a persisted non-default `ui_locale` overrides system detection, while `keeps first-launch system detection while the settings row is untouched` pins that the untouched seed row never overrides detection.
- `switching language preserves the current screen, filter, and nonsecret input` (`App.test.tsx`), `preserves the alias draft across a language switch but clears the secret` (`screens/ConnectionsScreen.test.tsx`), and `preserves non-secret form drafts across a language switch` (`screens/SettingsScreen.test.tsx`): locale switches keep the active screen, statistics filter, and non-secret drafts, while secret input is always cleared and never persisted.
- Bilingual coverage: the Connections and Settings flows assert both languages (English default and zh-CN strings after a switch), and `npm run i18n:check` enforces exact en/zh-CN parity (140 keys).

### Extra coverage beyond the criteria

- `malformed_settings_values_fail_safely` (Rust): an unparsable zone answers `validation` without side effects.
- `undocumented_lifecycle_transitions_are_refused` (Rust): enabling an active connection answers `validation`.
- `offers refresh Off explicitly alongside the intervals` (`screens/SettingsScreen.test.tsx`): the polling selector offers exactly `disabled`, `one_minute`, `five_minutes`, `fifteen_minutes` with Off labelled.
- `shows the session-only note in needs credentials and offers rotation` and `offers retry removal and a recovery note in deletion pending` (`screens/ConnectionsScreen.test.tsx`): recovery states render notes and actions.

## Validation commands (all run 2026-10-21, all exit 0)

| Command | Result |
| --- | --- |
| `./scripts/check-core-boundaries.sh` | pass — core calendar-only dependency boundary OK |
| `./scripts/test-check-core-boundaries.sh` | pass — boundary-gate regression checks passed |
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --locked --workspace` | pass — contracts 16, core 18, desktop 40 (15 new connection/settings tests + 25 P1-07), infrastructure 64 + 1 ignored, plus integration/doc suites; 0 failures |
| `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` | pass — "TypeScript contract is current" |
| `cargo build --locked -p quotavista-desktop` | pass |
| `apps/desktop`: `npm ci` | pass |
| `apps/desktop`: `npm run lint` | pass |
| `apps/desktop`: `npm run i18n:check` | pass — "Catalog parity OK: 140 keys, placeholders matching in both catalogs" |
| `apps/desktop`: `npm test` | pass — 65 tests (13 ConnectionsScreen + 10 SettingsScreen + 13 App + 12 ipc + i18n/Overview suites) |
| `apps/desktop`: `npm run typecheck` | pass |
| `apps/desktop`: `npm run build` | pass — Vite production build |

## Review round 1 fixes (2026-10-21, this branch)

A review of the initial PR found three Blocking and three Should-fix findings; all six are fixed on this branch with regression coverage. Evidence:

- **B1 — committed candidate deleted by the caller on cleanup failure.** `connect_provider`/`replace_credentials` no longer delete the candidate reference after `activate_credentials` errors: a pre-commit failure aborts inside the coordinator (journal-owned candidate cleanup), and the caller only removes the freshly created connection row/scope when the candidate never became live (never the reference). Regression: `rotation_cleanup_failure_keeps_the_activated_credential` (Rust) — with reference deletion failing, same-identity rotation answers a retryable `storage_error` while the activated credential stays readable (`SECRET_A` under the current reference), the superseded reference still resolves, the connection stays `active`, and after `recover()` completes the owed cleanup the connection accepts further mutations.
- **B2 — locale-only revision bump reset unsaved settings drafts.** `SettingsScreen` now resets drafts only when the persisted *content* changed (zone/currency/polling/thresholds), not on every revision bump. Regression: `preserves non-secret form drafts across a language switch` (renderer) now persists the locale through a revision-advancing write (revision 0 → 1) and asserts the unsaved zone draft survives the bump.
- **B3 — a stale locale retry could overwrite a newer committed choice.** Locale selections are fenced by a generation counter in `SettingsContext.selectLocale`; only the newest issued selection may reload and retry after a conflict. Regression: `an older in-flight locale write never overwrites a newer committed choice` (renderer) — the zh-CN write hangs, the newer en write commits, the older write then conflicts and neither retries, reloads, nor re-persists (exactly three `update_settings` calls total).
- **S4 — secrets cleared on submission, not after the response settles.** Both credential forms clear the field before any `await`; regression pair `clears the add secret on submission, before the request settles` and `clears the rotation secret on submission, before the request settles` (renderer) assert the empty field and the submitted payload while the request is still an unresolved promise.
- **S5 — nonzero revision no longer implied an explicit locale choice.** The first persistence of any preference records the displayed locale with it (`SettingsContext.save`); from then on a nonzero revision honestly reflects a locale the user saw. Regression: `records the displayed locale with the first persisted preference and stops pinning afterwards` (renderer) plus `applies a pinned locale from a first save after restart, even for non-locale changes` (App suite, restart semantics).
- **S6 — losing racing setup left a durable ghost `pending_setup` row.** The racing-candidates test now asserts exactly one connection row of any lifecycle remains (no ghost row/scope), and the backend removes the aborted setup's rows once compensation completes.

The overstated claims called out by the review were corrected: `secrets_never_leave_the_store` now scans every mutation envelope (Ok and Error) plus events, and `same_identity_rotation_preserves_history` asserts the retained usage snapshot. The full validation battery below was re-run after these fixes with the updated counts.

Rust test names above are exactly as they appear in the suites; the renderer test names match the `it(...)` titles in the cited files.

## Rebase onto merged P1-09 main (2026-10-21, this branch)

P1-09 merged to `main` (757facf, archive 60a4649) while this branch was in review. The branch was rebased onto the new `main`; conflicts in the shared additive files were resolved by keeping both sides' content:

- `apps/desktop/src-tauri/src/lib.rs`: P1-09's `SimulatedRouter` composition parameter and overview/statistics dispatch arms plus this task's `RoutingSecrets` parameter, connection arms, and startup reconciliation; the pre-merge placeholder test asserting overview/statistics were `UnsupportedInPhaseOne` was removed (its premise no longer holds; both command families are covered by the merged suites).
- `apps/desktop/src/i18n/{en,zh-CN}.json`: union of both tasks' keys (198 keys per catalog, parity re-checked); shared keys keep the merged `main` wording (e.g. `settings.reporting.zone` "报告时区"), and the test asserting that label was aligned to the merged catalog.
- `apps/desktop/src/App.test.tsx`: P1-09's envelope helpers/fixtures plus this task's settings routing in one `routeInvoke` default router; both suites' tests kept (74 renderer tests). The rebase splice initially dropped the `PROTOTYPE_ENVELOPE` fixture (runtime `ReferenceError` surfaced as "Backend status unavailable"); restored and re-verified.
- `apps/desktop/src-tauri/src/connections.rs`: new `JournalError::ResourceLimit` arm in `journal_error` (P1-09 added the variant).
- `docs/traceability.md`: both evidence sections kept (P1-09 before P1-10).

`git range-diff` against the pre-rebase branch: commits 2-5 patch-identical, commit 1 differs only in the conflict files above. The full validation battery (13 commands) was re-run on the rebased head — all exit 0, workspace test totals now include P1-09's suites (contracts 16, core 31, desktop 44, infrastructure 66 + 1 ignored, plus 13 + 13; renderer 74).

## Pending and not claimed

- Native store smoke check (real macOS Keychain round trip through the new flows): pending — every fixture uses the session-only in-memory fake; no native credential or real keychain entry was touched, and the extra-rules native smoke option was not needed for the criteria. To be exercised in native validation (P1-13) or on explicit request with a disposable `quotavista-test-` service entry.
- Native/WebView runtime behaviour (real window, real dialogs, tray): not claimed; renderer evidence is jsdom-only.
- Real Kimi/OpenRouter validation, managed backup purge/restore, alert-episode evaluation, multiple simultaneous accounts per provider: out of scope by PRD; the UI states explicitly that no provider validation occurs.
- P1-09 (overview/statistics) and P1-11 (desktop lifecycle) remain separate sibling tasks; nothing in their scope was implemented here.
