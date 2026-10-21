# P1-11 validation record

## Scope and prerequisites

This task adds desktop lifecycle and native-integration fallbacks on top of the
P1-07 restricted IPC composition. It does not add a parallel transport path: both
new commands travel through the existing single `dispatch_command` handler, the
existing `Request::validate` gate, and the existing sealed `Envelope`.

New code:

- `apps/desktop/src-tauri/src/lifecycle.rs` — close/wake/quit policy
  (`CloseDecision`, `RefreshMode`, `WakeDecision`, `WakeGate`, `handle_wake`,
  `shutdown`) plus 11 tests.
- `apps/desktop/src-tauri/src/platform.rs` — tray and notification capability
  adapters behind a `NotificationPort` (`PlatformState`, `render_notice`,
  `detect_tray`, `tray_disabled`, `TauriNotifications`) plus 9 tests.
- `apps/desktop/src-tauri/src/lib.rs` — two `dispatch` arms, one additive
  `AppState.platform` field, and a rewritten `run()` that registers the tray,
  the single-instance and notification plugins, and the close/reopen/quit/wake
  event handling. Existing logic was not reordered or reformatted: the diff
  against `origin/main` is 263 insertions and 9 deletions, and 5 of those 9
  deletions are one-word visibility widenings (`pub(crate)`) on existing test
  helpers so the new module tests reuse the P1-07 fixtures instead of
  duplicating them.
- `crates/contracts/src/lib.rs` — 1 new scalar, 3 new wire enums, 2 new DTOs, 2
  new `Command`/`CommandResult` variants, appended to the `typescript()` emit
  list. `Request::validate` needed no change because its command match already
  ends in a catch-all arm.
- `apps/desktop/src/components/DesktopIntegration.tsx` (+ test) — the visible
  capability region, mounted as a footer in `App.tsx`; 17 new i18n keys in both
  catalogs; 2 new typed client methods in `ipc.ts`; 2 new compile fixtures in
  `contracts.typecheck.ts`; a scoped `.desktop-integration*` CSS block appended
  after the last existing rule.
- `scripts/native-checks.sh` — the reproducible native procedure for this host
  and for P1-13.

Prerequisites confirmed with git before activation. `HEAD` equalled
`origin/main` at `44a0f33` (`git rev-list --left-right --count
origin/main...HEAD` printed `0	0`). P1-07 is merged (PR #8, head `9188637`,
merge commit `34fc4c3`) and archived at
`.trellis/tasks/archive/2026-10/10-18-p1-07-ipc` by `44a0f33`; P1-08 is
archived at `.trellis/tasks/archive/2026-10/10-18-p1-08-bilingual-shell` by
`a54b56b`. All four commits are ancestors of `HEAD`. The three stale links in
this task's `prd.md`, `design.md` and `implement.md` were repointed at the
archive paths.

Activation was recorded with
`python3 .trellis/scripts/task.py start .trellis/tasks/10-18-p1-11-desktop-lifecycle`
under a session-scoped `TRELLIS_CONTEXT_ID` that is not committed: status moved
`planning → in_progress` and the branch `jondong/p1-11-desktop-lifecycle` was
recorded in `task.json`.

Environment: macOS 27.0.1 (26A434) arm64, session type n/a (Aqua), rustc
1.99.1, cargo 1.99.1, Node v24.22.0, npm 11.19.1, Tauri 2.12.4,
`tauri-plugin-notification` 2.7.0, `tauri-plugin-single-instance` 2.5.4.

Safety: every fixture is synthetic and throwaway. Rust tests use `TempDir`
roots, the in-memory session secret store chosen explicitly, and the in-repo
simulated adapter; the renderer tests run in jsdom against a mocked `invoke`.
The native smoke runs used a fresh `mktemp -d` `TMPDIR` and
`QUOTAVISTA_SECRET_STORE=session-only`, so no real keychain or Secret Service
entry was read, written or deleted and no `quotavista-test-` service was needed.
The only directory the prototype wrote to was
`$TMPDIR/quotavista-prototype/{journal,quotavista.sqlite,quotavista.sqlite-shm,quotavista.sqlite-wal}`.
No provider endpoint, production database or real application data directory was
contacted. Every smoke run ended with `leftover=0` processes and its temp
directory removed.

## Repository validation

All commands were run from the repository root unless stated, and all passed.

| Command | Result |
| --- | --- |
| `./scripts/check-core-boundaries.sh` | exit 0, `quotavista-core calendar-only dependencies: boundary OK` |
| `./scripts/test-check-core-boundaries.sh` | exit 0, 5 × `ok`, `all boundary-gate regression checks passed` |
| `cargo fmt --all -- --check` | exit 0, no diff |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0, 0 errors |
| `cargo test --locked --workspace` | exit 0, 163 passed, 0 failed, 1 ignored |
| `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` | exit 0, `TypeScript contract is current` |
| `cargo build --locked -p quotavista-desktop` | exit 0, `Finished` |
| `git diff --check` | exit 0, no whitespace errors |
| `apps/desktop: npm ci` | exit 0 |
| `apps/desktop: npm run lint` | exit 0, `ESLint: No issues found` |
| `apps/desktop: npm run i18n:check` | exit 0, `Catalog parity OK: 75 keys, placeholders matching in both catalogs` |
| `apps/desktop: npm test` | exit 0, 5 files passed, 47 tests passed |
| `apps/desktop: npm run typecheck` | exit 0 |
| `apps/desktop: npm run build` | exit 0, `✓ built`, `dist/assets/index-B84mkkpi.js 246.69 kB` |
| `./scripts/native-checks.sh --environment` | exit 0, prints the host block quoted above |
| `./scripts/native-checks.sh --automated` | exit 0, `AUTOMATED: PASS` |

`cargo fmt --all` had to be run before the `--check` gate: it reformatted six
spots in the new code. Clippy's only remaining output is the pre-existing
ts-rs `failed to parse serde attribute` noise about `deny_unknown_fields`, which
`-D warnings` does not promote and which is present on `origin/main` too.

Test counts moved from 143 to 163 in Rust (desktop crate 22 → 42: 11 lifecycle
plus 9 platform) and from 40 to 47 in the renderer (7 new
`DesktopIntegration.test.tsx` cases). `./scripts/native-checks.sh --automated`
reports three labelled steps, each from the real exit status of its command:
`RESULT: PASS (lifecycle policy: close, wake, Off mode, quit)`,
`RESULT: PASS (platform capability adapters: tray and notification detection)`,
`RESULT: PASS (IPC boundary still sealed)`.

## Criterion 1 — no tray means closing exits, with no orphan collector

> Without a usable tray, closing the main window exits; there is no inaccessible
> orphan collector. All essential flows remain reachable from the main window.

The policy is a pure function of the detected capability, not of the desktop
environment: `close_decision(TrayAvailabilityDto::Unavailable)` returns
`CloseDecision::ExitApplication` and `Available` returns `HideToTray`.
Proof: `lifecycle::tests::closing_the_main_window_exits_when_no_tray_was_detected`.

In `run()`, `WindowEvent::CloseRequested` calls `api.prevent_close()` and hides
the window only for `HideToTray`; for `ExitApplication` it does nothing, so the
close proceeds. That the close then really terminates the process is a property
of the runtime, and it was verified in the pinned dependency source rather than
assumed: `tauri-runtime-wry-2.12.2/src/lib.rs:4257-4273` handles
`TaoWindowEvent::Destroyed` by removing the window and, when the window map
becomes empty, emitting `RunEvent::ExitRequested { code: None }` and setting
`ControlFlow::Exit` unless a handler prevents it. That arm is not macOS-gated.
`on_close_requested` at `:4351-4376` dispatches `CloseRequested` to the handler
and only proceeds with the close when no prevention was signalled, so
`prevent_close` is what keeps the tray case alive.

The one `ExitRequested { code: None }` handler therefore covers both a
user-initiated quit and a no-tray window close: it prevents the default exit,
runs `lifecycle::shutdown`, and calls `handle.exit(0)`. That re-enters the loop
with `code: Some(0)`, which the `code.is_none()` guard leaves alone, so
`ControlFlow::Exit` proceeds and the handler cannot loop.

Reachability from the main window is asserted in the renderer: the desktop
region is a footer inside the shell, the four P1-08 navigation entries are
unchanged, and the region issues no IPC call until the user presses a button.
Proof: `DesktopIntegration.test.tsx` — `reads nothing until the user asks, then
reports both capabilities` and `explains that closing the window exits when no
tray was detected`; `App.test.tsx` still passes unmodified (47 renderer tests),
which is the evidence that nothing was taken away from the existing shell.

The no-tray path is reproducible on this host without removing the menu bar:
`QUOTAVISTA_TRAY=off` short-circuits registration so no icon is created at all
and the application reports `tray=Unavailable`. The override is one-way — it can
only remove a capability, never add one — so it cannot manufacture a false
"the tray works" result. Proof:
`platform::tests::the_tray_override_only_accepts_an_exact_off`.

## Criterion 2 — with a tray, reopen and explicit quit

> With a usable tray, users can reopen and explicitly quit. Quit cancels work and
> commits or rolls back active writes.

Reopen and quit are tray **menu items**, not tray mouse events, because the
official Tauri documentation states that on Linux the tray mouse event "is not
emitted even though the icon is shown and will still show a context menu on
right click". An entry point built on clicks would work on macOS and silently do
nothing on Linux. The menu carries `Show QuotaVista` and `Quit`; a Linux tray
icon is also sometimes invisible until a menu is attached, so the menu is always
set.

`quit` runs `lifecycle::shutdown`, which locks the refresh coordinator, calls
`cancel` on every live job so each one reaches a terminal outcome, records the
report, and then closes the SQLite pools. It deliberately drives no `pump`, so
quitting cannot commit anything new, and it publishes no change event — each
command path announces its own commits. Proof:
`lifecycle::tests::quit_cancels_the_work_a_wake_left_in_flight` (24 connections
admitted, `running() + pending() > 0` before, `cancelled` non-empty and both
counters 0 after, every job id has an outcome, event count unchanged),
`lifecycle::tests::quitting_twice_is_safe`, and
`lifecycle::tests::quit_leaves_committed_data_readable_after_a_restart` (a fresh
composition on the same root reads back the connection and the committed
revision).

Commit-or-roll-back is a property of the P1-06 coordinator that shutdown relies
on rather than reimplements: `cancel` sets `Outcome::Cancelled` on a job that has
not committed, and a job that already published keeps its commit because
publication is atomic per candidate. A job abandoned mid-history leaves its
candidate unpublished, so the previously committed partition is untouched.
Proof: `quit_leaves_committed_data_readable_after_a_restart` for the committed
side, and the pre-existing `a_refused_commit_keeps_the_cached_data_and_announces_no_revision`
for the uncommitted side.

## Criterion 3 — synthetic notification, localized, no account data

> Synthetic notification uses the active locale and no identifying account
> details; unavailable delivery retains an in-app status. It does not establish
> real alert behavior.

`render_notice` is the only notification-text catalog in the product and is
keyed by `(template_key, locale)`, not by a translated episode identity, so a
locale switch re-renders the same template without creating an alert episode and
future threshold or expiration alerts add their own keys. The single
`NotificationTemplateKey::SyntheticProbe` variant is the whole enum today.
Proof: `platform::tests::every_template_renders_in_both_locales_with_distinct_bounded_text`
(non-empty, at most 200 characters, no control characters, and the en and zh-CN
titles and bodies differ) and
`platform::tests::rendered_notification_text_names_no_account_or_credential`
(the lowercased title and body contain none of `api`, `key`, `token`, `secret`,
`credential`, `password`, `bearer`, `kimi`, `openrouter`, or any fixture
identifier).

Text is bounded through `NotificationTextDto::try_from`, and a bound violation
becomes a typed `internal_error` rather than a panic inside a command handler.

Unavailable delivery keeps an in-app status: `PlatformState::preview` returns the
rendered text on every path, with `delivered: false` and the detected capability
echoed back when the channel refused. Proof:
`platform::tests::an_unusable_channel_keeps_the_explanation_in_the_application`
(both `Unsupported` and `PermissionDenied`),
`platform::tests::an_available_channel_reports_the_text_it_accepted`, and
`platform::tests::a_channel_that_refuses_at_delivery_time_reports_why` (a channel
that detected as available but refuses at delivery time reports the refusal
instead of a false success). In the renderer: `keeps an in-app explanation when
delivery is unavailable` and `explains a failed transport without pretending the
notification went out`.

This does not establish real alert behaviour. No episode is persisted, nothing is
sent on a timer, on launch or on wake, and the renderer sends nothing until the
user presses the button (`sends the synthetic notification in the active locale
and reports acceptance`, `asks for the notification in the locale the user
selected`, which asserts the exact wire shape
`{ command: "preview_system_notification", args: { template_key: "synthetic_probe", locale: "zh-CN" } }`).

One honesty limit is recorded in the adapter's own documentation and in
`.trellis/spec/backend/error-handling.md`: `tauri-plugin-notification`'s desktop
`show()` spawns the delivery and discards the operating-system result
(`src/desktop.rs:434-441` on Windows and macOS, `:448-467` on Linux), and its
desktop `permission_state()` and `request_permission()` hard-code
`Ok(PermissionState::Granted)` (`src/desktop.rs:196-206`, "Desktop applications
do not need to ask for this permission"). So `delivered: true` means only that
the platform channel accepted the request, never that a banner was seen. The
renderer says exactly that: `desktop.notification.delivered` reads "The platform
accepted the notification."

## Criterion 4 — wake merges permitted work, Off starts nothing

> Wake merges permitted refresh work; refresh-Off never initiates an implicit
> query. Record differences between available native targets.

Tauri 2.12 has no desktop suspend or resume event: `WindowEvent::Suspended` and
`Resumed` are `#[cfg(mobile)]` (`tauri-2.12.4/src/app.rs:111-170`). Wake is
therefore derived from reactivation, and each source is named in the report so
the native targets can be compared: `WindowFocused`,
`ReopenWithoutVisibleWindow` (macOS only — `RunEvent::Reopen` is
`#[cfg(target_os = "macos")]`, `src/app.rs:275-283`), and
`SingleInstanceActivation`. **This is the recorded difference between available
native targets**: macOS has two reactivation signals plus the handover, Linux has
the focus signal plus the handover and no `Reopen`.

`handle_wake` reads the persisted settings first. With `refresh_minutes: None`
it returns before the coordinator is touched, so no job is created and no
provider can be contacted; the shipped default is `NULL`
(`crates/infrastructure/migrations/0001_persistence.sql:16`). Proof:
`lifecycle::tests::refresh_off_never_initiates_an_implicit_query_on_wake`
(`permitted` false, `launched` empty, `refused` 0, `job_ids()` empty,
`running()` and `pending()` both 0, revision unchanged, no change event, and the
diagnostic names no connection) and
`lifecycle::tests::the_coordinator_also_refuses_a_scheduled_refresh_while_off`
(the coordinator's own `tick` returns `Admission::Refused`, so the guarantee does
not rest on one layer).

With an interval configured the wake asks the coordinator for one merged refresh
and the coordinator decides per connection whether the interval has elapsed, so a
wake never replays missed timers. Proof:
`lifecycle::tests::wake_merges_one_refresh_and_a_second_wake_adds_nothing` (first
wake launches 1 job, advances the revision and emits exactly one event; second
wake is permitted but launches nothing, reports `refused == 1`, and leaves both
the revision and the event count alone) and
`lifecycle::tests::wake_is_permitted_only_when_automatic_refresh_is_on`.

Reactivation signals arrive in bursts — showing a hidden window raises a focus
event, and a handover raises both — so `WakeGate` collapses them on a monotonic
clock; a backwards clock reading counts as an immediate repeat and is refused.
Proof: `lifecycle::tests::the_wake_gate_swallows_a_burst_of_reactivation_signals`
and `lifecycle::tests::the_wake_gate_turns_a_reactivation_burst_into_one_wake`.

Wake is wired into the same commit-announcement path as the refresh command: it
captures the revision before, pumps within `MAX_REFRESH_STEPS`, and calls
`publish_change`, which emits only when `store_state.data_revision` actually
advanced. Diagnostics print counts only (`wake <source> mode <mode> permitted
<bool> merged <n> not-due <n>`), never a connection identifier, which keeps them
inside the logging allowlist.

Secret-store recovery ordering is preserved and extended: `build_inner` still
awaits `coordinator.recover()` before opening the refresh coordinator, and a
journal that cannot be decoded now aborts the whole desktop composition, so no
scheduler exists at all. Proof:
`lifecycle::tests::a_corrupt_lifecycle_journal_blocks_the_composition_before_scheduling`
(`build_with_provider` fails with `internal_error`, the journal file is left
byte-identical, and no event was published).

## Native checks observed on this host

Three smoke runs of `target/debug/quotavista-desktop`, each with a fresh
`mktemp -d` `TMPDIR` and `QUOTAVISTA_SECRET_STORE=session-only`. stderr is
quoted verbatim.

1. **Default run.** `quotavista: capabilities tray=Available
   notification=Available`. The process stayed alive. The data directory
   contained only `quotavista-prototype/{journal, quotavista.sqlite,
   quotavista.sqlite-shm, quotavista.sqlite-wal}`. A second launch while the
   first was alive exited immediately with status 0 and empty stderr while the
   first kept running: single-instance behaviour observed for real. This run also
   exposed a genuine defect — see below.
2. **Default run after the fix.** `quotavista: capabilities tray=Available
   notification=Available` then `quotavista: wake SingleInstanceActivation mode
   Off permitted false merged 0 not-due 0`, with no panic. Second instance
   `alive=no`, first `alive=yes`, `leftover=0` after cleanup. This is native
   evidence that the handover reaches the wake path and that refresh-Off starts
   nothing on a real wake.
3. **No-tray run.** `QUOTAVISTA_TRAY=off` printed `quotavista: capabilities
   tray=Unavailable notification=Available`; the process launched and stayed
   alive; `leftover=0` after cleanup.

**Defect found and fixed by the native run.** The first run panicked with
`thread 'tokio-rt-worker' panicked at apps/desktop/src-tauri/src/lib.rs:847:19:
Cannot start a runtime from within a runtime.` The cause is in the pinned plugin:
`tauri-plugin-single-instance-2.5.4/src/platform_impl/macos.rs:102` does
`tauri::async_runtime::spawn(async move { … })` and calls the handler at `:113`,
so the single-instance callback runs on a tauri async-runtime worker thread, not
the main thread, and `Handle::block_on` there panics. `show_main_window` had
already run, so only the wake half died — which is why the window still came to
the front and the failure was easy to miss. The callback now marshals onto the
main thread with `run_on_main_thread`. The other callbacks were checked and need
no change: the tray `on_menu_event` handler is drained at
`tauri-2.12.4/src/app.rs:2755` from `EventLoopMessage::MenuEvent`, tray icon
events via `src/manager/tray.rs:44`, and `WindowEvent`, `Reopen` and
`ExitRequested` are all main-thread, so `block_on` is safe in every one of them.
Using `tauri::async_runtime::spawn` instead was rejected because the SQLite pools
in `AppState` are bound to the runtime built in `run()`.

## Native targets recorded as pending

GUI automation is **not** genuinely available in this environment, so the
interactive steps are recorded as pending rather than passed. Probes against the
running application returned `count windows` = `0`; asking for `window 1` failed
with `-1719` (`无效的索引`); `{frontmost, visible}` was `{false, true}`;
`set frontmost to true` produced no effect and `keystroke "q" using command down`
never reached the process. Process enumeration does work, so Apple Events are
permitted but UI scripting of this application's windows is not.

`./scripts/native-checks.sh --interactive` therefore reports 13 ×
`OBSERVED: PENDING (no interactive terminal; run this step manually)` and never
guesses a pass. The pending steps are: cold launch, in-app capability read,
English notification banner, Simplified-Chinese notification banner, close with a
tray hides, reopen from the tray, explicit quit from the tray, no-tray close
exits, single instance, wake merges refresh, refresh-Off starts nothing, quit
cancels in-flight work, restart keeps committed data.

Also pending, with the reason:

- **Ubuntu 26.04 GNOME x86_64.** No Linux host is available here, so no GNOME
  compatibility claim is made. `scripts/native-checks.sh --environment` records
  the distribution, `XDG_SESSION_TYPE` (Wayland or X11), `XDG_CURRENT_DESKTOP`,
  the GNOME Shell version, and a `gdbus` probe of
  `org.kde.StatusNotifierWatcher.IsStatusNotifierHostRegistered`.
- **Linux tray availability.** `detect_tray` reports `Unavailable` on Linux even
  after a successful registration, because `tray-icon-0.25.1`'s default features
  are `["muda-libxdo", "libappindicator"]` and its `AppIndicator::new` plus
  `set_status(Active)` succeed with no indicator host running
  (`src/platform_impl/gtk/mod.rs:24-53`). Registration is therefore not proof of
  a visible icon. Promoting Linux to `Available` needs a host probe validated by
  a native run, which is P1-13 work; until then the conservative answer keeps the
  no-tray close policy, which is the safe direction.
- **Real sleep/wake.** No desktop wake event exists in Tauri 2.12, and a real
  suspend/resume cycle cannot be observed from this session. The behaviour is
  covered by the reactivation sources above and by the recorded native
  single-instance wake.
- **Notification permission state.** Not a detection on desktop: the plugin
  always answers granted. `notification=Available` in the runs above means the
  plugin is registered, nothing more.
- **macOS notification attribution.** In a dev build the plugin attributes
  notifications to `com.apple.Terminal` (`src/desktop.rs:518-524`), so a banner
  observed from `cargo run` is not attributed to QuotaVista. A bundled build is
  needed for that, which is out of scope here.
- **Bilingual tray labels.** The tray is registered once at start-up while locale
  switching lives in the renderer, so localized tray labels would need a
  settings-driven re-registration. The labels stay English and this is recorded
  rather than faked; the notification text *is* localized because it is rendered
  per request.

## Technical choices checked against official documentation

Verified against v2.tauri.app, docs.rs/tauri/2.12.4, and the vendored crate
sources under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`:

- The tray needs the `tray-icon` Cargo feature; `tauri::tray` is desktop- and
  feature-gated and `RunEvent::TrayIconEvent` is
  `#[cfg(all(desktop, feature = "tray-icon"))]`. The feature was added to
  `apps/desktop/src-tauri/Cargo.toml`.
- `tauri-plugin-single-instance` must be registered first, and it is.
- `NotificationExt::notification()` calls `state::<Notification<R>>()` and panics
  when the plugin is absent, so `TauriNotifications::detect` probes with
  `try_state` first and reports `Unsupported` when it is missing — which is also
  what every test composition gets, since tests build `AppState` directly and
  never register a plugin.
- `app.default_window_icon()` falls back to `icons/icon.png` when
  `bundle.icon` is empty (`tauri-codegen-2.7.3/src/context.rs:211-226` and
  `:479-493`), and that file exists, so the tray icon needs no
  `tauri.conf.json` change.

**Permissions.** Both plugins are registered from Rust only and no capability
permission was added. `apps/desktop/src-tauri/capabilities/default.json` is
byte-identical to `origin/main` and still lists exactly
`allow-dispatch-command`, `core:event:allow-listen` and
`core:event:allow-unlisten`. The `gen/schemas/*.json` changes are build-generated
*enumerations* of the identifiers the plugins make available, written by
`tauri-build` for editor autocompletion; they grant nothing. The renderer still
reaches the composition exclusively through `dispatch_command`, and
`tests::the_generated_handler_accepts_the_renderer_argument_shape` and
`tests::the_generated_handler_answers_a_missing_argument_with_an_error_envelope`
still pass, so the IPC boundary is unchanged.

## Documentation updates

- `docs/traceability.md` — a `## P1-11 prototype evidence` section was inserted
  before the release evidence rules. No criterion status was changed: A07, A11,
  A21 and A25 all remain `Pending`, because prototype tests contribute evidence
  without closing a release criterion. The introductory paragraph was left alone,
  following the P1-08 precedent of having an evidence section without a sentence
  there, and to keep a heavily contended shared line stable for the parallel
  P1-09 and P1-10 branches.
- `.trellis/spec/backend/error-handling.md` — one appended paragraph recording
  the rule this task established: a capability that cannot report failure must
  not be surfaced as success, and a successful registration is not proof of a
  usable surface.
- `scripts/native-checks.sh` — new, executable, `bash -n` clean.
- No other spec, requirement or architecture document was changed, and no sibling
  task directory was touched.

## Open items

- All 13 interactive native steps above, on macOS and on Ubuntu 26.04 GNOME.
- Linux tray detection stays conservatively `Unavailable` until a host probe is
  validated by a native run (P1-13).
- `delivered: true` cannot mean the user saw a banner; a delivery receipt would
  need the plugin's tracked-notification path and is out of scope.
- Bilingual tray labels need settings-driven re-registration.
- P1-09 and P1-10 own the overview, statistics, connections and settings screens.
  This branch stays additive in the shared files (`lib.rs`, `ipc.ts`,
  `contracts`, catalogs, `Cargo.lock`, `traceability.md`) so their merges stay
  independent. `Cargo.lock` was resolved with `cargo fetch` rather than
  `cargo generate-lockfile` to avoid bumping six unrelated packages.
