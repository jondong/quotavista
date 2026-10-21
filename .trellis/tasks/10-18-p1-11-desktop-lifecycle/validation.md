# P1-11 validation record

## Scope and prerequisites

This task adds desktop lifecycle and native-integration fallbacks on top of the
P1-07 restricted IPC composition. It does not add a parallel transport path: both
new commands travel through the existing single `dispatch_command` handler, the
existing `Request::validate` gate, and the existing sealed `Envelope`.

New code:

- `apps/desktop/src-tauri/src/lifecycle.rs` (1162 lines) — close/wake/quit policy
  (`CloseDecision`, `RefreshMode`, `WakeDecision`, `WakeGate`, `handle_wake`,
  `handle_gated_wake`, `shutdown`, `ShutdownSignal`, `ResumeMonitor`,
  `drive_resume_monitor`) plus 21 tests.
- `apps/desktop/src-tauri/src/platform.rs` (657 lines) — tray and notification
  capability adapters behind a `NotificationPort` (`PlatformState`,
  `render_notice`, `detect_tray`, `tray_disabled`, `capability_from_plugin`,
  `delivery_from_plugin`, `TauriNotifications`) plus 13 tests.
- `apps/desktop/src-tauri/src/lib.rs` — two `dispatch` arms, two additive
  `AppState` fields (`platform`, `shutdown_signal`), a rewritten `run()` that
  registers the tray, the single-instance and notification plugins, spawns the
  resume monitor and handles close/reopen/quit/wake, plus the non-blocking
  `wake`/`quit` callbacks and one new test. Existing logic was not reordered or
  reformatted: the diff against `origin/main` is 401 insertions and 10 deletions,
  and every one of the 10 deletions is accounted for: 5 are one-word visibility
  widenings (`pub(crate)`) on existing test helpers so the new module tests reuse
  the P1-07 fixtures instead of duplicating them, 4 are lines inside `run()` that
  the rewrite replaced (the empty `app.run` callback and three builder lines), and
  1 is P1-09's `runtime.block_on(seed_demo_connections(&state))` switched to the
  `setup_runtime` handle clone.
- `crates/contracts/src/lib.rs` — 1 new scalar, 4 new wire enums, 2 new DTOs, 2
  new `Command`/`CommandResult` variants, appended to the `typescript()` emit
  list (72 insertions). `Request::validate` needed no change because its command
  match already ends in a catch-all arm.
- `apps/desktop/src/components/DesktopIntegration.tsx` (177 lines) and its test
  (272 lines, 8 cases) — the visible capability region, mounted as a footer in
  `App.tsx`; 19 `desktop.*` keys in both catalogs (122 keys each after P1-09's
  merge); 2 new typed client methods in `ipc.ts`; 2 new compile fixtures in
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
archive paths. The branch was later rebased onto `origin/main` at `60a4649`
after P1-09 merged as `757facf`; both conflicts were additive and were resolved by
keeping both sides, as recorded under Open items.

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

All commands were run from the repository root unless stated, and all passed. The
results below are the post-rebase measurement, taken on the branch head after the
round-1 review fixes and the rebase onto P1-09.

| Command | Result |
| --- | --- |
| `./scripts/check-core-boundaries.sh` | exit 0, `quotavista-core calendar-only dependencies: boundary OK` |
| `./scripts/test-check-core-boundaries.sh` | exit 0, 5 × `ok`, `all boundary-gate regression checks passed` |
| `cargo fmt --all -- --check` | exit 0, no diff |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0, 0 errors |
| `cargo test --locked --workspace` | exit 0, 201 passed, 0 failed, 1 ignored |
| `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` | exit 0, `TypeScript contract is current` |
| `cargo build --locked -p quotavista-desktop` | exit 0, `Finished` |
| `git diff --check` | exit 0, no whitespace errors |
| `apps/desktop: npm ci` | exit 0 |
| `apps/desktop: npm run lint` | exit 0, `ESLint: No issues found` |
| `apps/desktop: npm run i18n:check` | exit 0, `Catalog parity OK: 122 keys, placeholders matching in both catalogs` |
| `apps/desktop: npm test` | exit 0, 6 files passed, 57 tests passed |
| `apps/desktop: npm run typecheck` | exit 0 |
| `apps/desktop: npm run build` | exit 0, `✓ built`, `dist/assets/index-BEolSCxv.js 260.61 kB` |
| `./scripts/native-checks.sh --environment` | exit 0, prints the host block quoted above |
| `./scripts/native-checks.sh --automated` | exit 0, `AUTOMATED: PASS` |

`cargo fmt --all` had to be run before the `--check` gate: it reformatted six
spots in the new code. Clippy's only remaining output is the pre-existing
ts-rs `failed to parse serde attribute` noise about `deny_unknown_fields`, which
`-D warnings` does not promote and which is present on `origin/main` too.

These counts are measured on the rebased head, so they include the tests P1-09
merged into `main`. The workspace runs 201 Rust tests: 16 in
`contracts/tests/contracts.rs`, 31 in `quotavista_core`, 62 in
`quotavista_desktop_lib`, 66 plus 1 ignored in `quotavista_infrastructure`, 13 in
`quotavista_providers`, and 13 contract doc-tests. The desktop crate's 62 split
into 21 `lifecycle::tests`, 13 `platform::tests` and 28 crate-root tests — 22
inherited from P1-07, 5 from P1-09's statistics module, and one added by this
task. The renderer runs 57 tests across 6 files, of which
`DesktopIntegration.test.tsx` contributes 8.

`./scripts/native-checks.sh --automated` reports three labelled steps, each from
the real exit status of its command:
`RESULT: PASS (lifecycle policy: close, wake, Off mode, quit)`,
`RESULT: PASS (platform capability adapters: tray and notification detection)`,
`RESULT: PASS (IPC boundary still sealed)`.

Two regressions surfaced only when the rebased tree was compiled, and both are
recorded here because a rebase that passes `git rebase --continue` can still fail
to build. P1-09's `run()` calls `runtime.block_on(seed_demo_connections(&state))`
inside the `setup` closure, which moved the owned runtime and left
`app.run`'s closure referring to it (`error[E0382]: use of moved value:
runtime`); the call now uses the `setup_runtime` handle clone that the same
closure already uses for `build_for_tauri`, so `runtime` stays free for the event
loop. Separately, a test-module import of `refresh::MAX_CONCURRENCY` was missing
(`error[E0425]`); it is imported inside `mod tests` rather than at module level so
a non-test build does not see an unused import.

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
tray was detected`; `App.test.tsx` still passes unmodified (57 renderer tests),
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

Delivery is reported in three states instead of one boolean, because the pinned
plugin can only ever say one of them honestly. `DeliveryOutcome::Delivered` is
reserved for an adapter that read the platform's own answer;
`DeliveryOutcome::Queued` is what the desktop plugin produces, since `show()`
hands the notice to an asynchronous task and returns `Ok` without the result;
`DeliveryOutcome::Refused(capability)` carries the reason.
`PlatformState::preview` maps only a confirmed delivery to
`NotificationCapabilityDto::Available` — a queued one keeps whatever was
detected, so it can never upgrade an unconfirmed channel into a confirmed one —
and returns the rendered text on every path, so the interface always has
something to show. Proof:
`platform::tests::only_a_confirmed_delivery_reports_delivered` (each of the three
outcomes maps to its own `NotificationPreviewDto.delivery`),
`platform::tests::a_queued_notification_stays_unconfirmed_and_keeps_its_text`
(the capability stays `Unconfirmed` next to `delivery == Queued`, and the zh-CN
text still reaches the caller),
`platform::tests::an_unusable_channel_keeps_the_explanation_in_the_application`
(both `Unsupported` and `PermissionDenied`),
`platform::tests::a_channel_that_confirms_delivery_reports_delivered_and_available`,
and `platform::tests::a_channel_that_refuses_at_delivery_time_reports_why` (a
channel that detected as available but refuses at delivery time reports the
refusal instead of a false success). In the renderer: `keeps an in-app
explanation when delivery is unavailable`, `reports a queued notification as
unconfirmed rather than accepted`, and `explains a failed transport without
pretending the notification went out`.

This does not establish real alert behaviour. No episode is persisted, nothing is
sent on a timer, on launch or on wake, and the renderer sends nothing until the
user presses the button (`sends the synthetic notification in the active locale
and reports confirmed delivery`, `asks for the notification in the locale the user
selected`, which asserts the exact wire shape
`{ command: "preview_system_notification", args: { template_key: "synthetic_probe", locale: "zh-CN" } }`).

The real adapter's two mappings are pure functions so they can be tested without
a window server and without sending anything to the operating system:
`capability_from_plugin(registered, permission_granted)` and
`delivery_from_plugin(shown)`. Proof:
`platform::tests::a_plugin_that_answers_granted_is_reported_as_unconfirmed`
(registered plus granted yields `Unconfirmed`, not `Available`, and an
unregistered plugin yields `Unsupported` even when something claims it was
granted) and
`platform::tests::plugin_success_is_queued_because_the_os_result_is_never_read`.
The limits those functions encode are documented in the adapter and in
`.trellis/spec/backend/error-handling.md`: `tauri-plugin-notification`'s desktop
`show()` spawns the delivery and discards the operating-system result
(`src/desktop.rs:434-441` on Windows and macOS, `:448-467` on Linux), and its
desktop `permission_state()` and `request_permission()` hard-code
`Ok(PermissionState::Granted)` (`src/desktop.rs:196-206`, "Desktop applications
do not need to ask for this permission"). A native macOS launch therefore prints
`notification=Unconfirmed`, and the renderer says so in both catalogs:
`desktop.notification.unconfirmed` reads "Channel present, delivery unconfirmed"
and `desktop.notification.queued` reads "Handed to the platform, which never
reported back. The message is repeated here." `desktop.notification.delivered`
now reads "The operating system confirmed the notification." and is unreachable
from the pinned desktop adapter. Detecting a real Linux notification daemon is
left to P1-13: `scripts/native-checks.sh --environment` probes
`org.freedesktop.Notifications.GetCapabilities` over the session bus on Linux and
records the answer, because a probe this host cannot validate natively must not
ship as a guess.

## Criterion 4 — wake merges permitted work, Off starts nothing

> Wake merges permitted refresh work; refresh-Off never initiates an implicit
> query. Record differences between available native targets.

Tauri 2.12 emits no desktop suspend or resume event: `WindowEvent::Suspended` and
`Resumed` are `#[cfg(mobile)]` (`tauri-2.12.4/src/app.rs:111-170`). Two
independent sources therefore feed the wake path, and each is named in the report
so the native targets can be compared.

The first is reactivation: `WindowFocused`, `ReopenWithoutVisibleWindow` (macOS
only — `RunEvent::Reopen` is `#[cfg(target_os = "macos")]`, `src/app.rs:275-283`)
and `SingleInstanceActivation`. **This is the recorded difference between
available native targets**: macOS has two reactivation signals plus the handover,
Linux has the focus signal plus the handover and no `Reopen`.

The second is `ResumeMonitor`, the actual sleep/wake adapter, which does not
depend on window activation at all. Reactivation alone would miss a resume that
happens while the main window stays hidden in the tray, because nothing is
focused and nothing is reopened. `run()` spawns `drive_resume_monitor` on the
composition runtime; it samples a monotonic and a wall clock every `RESUME_TICK`
(30s) and treats an advance of at least `RESUME_THRESHOLD` (120s) as a suspend
gap, reporting `WakeSource::ResumeAfterSuspend`. Both clocks are compared and
either one advancing is enough, because Rust's `Instant` is `mach_absolute_time`
on macOS (which does not advance during sleep) but `CLOCK_MONOTONIC` on Linux,
whose behaviour across suspend varies by kernel, while the wall clock always
advances. A wall-clock step from NTP or a manual change is at worst one extra
wake, still governed by the refresh mode, the coordinator's own interval and the
gate; `saturating_sub` makes a backwards clock look like no time at all, so it can
never trigger a refresh. The first sample only establishes a baseline. Proof:
`lifecycle::tests::a_suspend_gap_is_detected_from_either_clock`,
`lifecycle::tests::the_first_resume_sample_only_establishes_a_baseline`,
`lifecycle::tests::ordinary_ticks_and_a_backwards_clock_are_not_a_resume`,
`lifecycle::tests::a_detected_resume_merges_one_refresh_without_window_activation`
(a resume with no focus and no reopen still merges exactly one refresh, and a
second gap inside the gate interval adds nothing) and
`lifecycle::tests::a_detected_resume_starts_nothing_while_refresh_is_off` (the
Off gate holds on the resume path too). A real machine suspend is still pending
`scripts/native-checks.sh --interactive` step 14; what is proven here is the
detection mechanism and that it reaches the same gated wake path.

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

Lifecycle work never blocks the event-loop thread. `wake` and `quit` only clone an
`AppHandle` and `runtime.spawn`; the settings read, the admission writes and the
coordinator pump all run on a composition-runtime worker, so a contended database
or a slow provider cannot freeze window interaction or the user's Quit action,
which is what `docs/architecture.md` requires of database work.
`AppState.shutdown_signal` ties the two together: `shutdown` raises it before it
waits for the refresh lock, `handle_gated_wake` checks it before the gate so a
wake arriving while quitting neither consumes the gate's slot nor competes for
that lock, and `handle_wake`'s bounded pump loop checks it between steps so it
releases the lock instead of running the remaining steps. `quit` exits the process
from inside its own task, after cancellation and the pool close, so "commit or
roll back active writes" still holds at the process boundary. Proof:
`tests::the_wake_callback_returns_while_the_refresh_pipeline_is_still_busy` (a
synthetic holder owns the refresh lock for 400ms; the real `wake` callback returns
in under 100ms, and shutdown still completes with nothing running, nothing
pending and no event published),
`lifecycle::tests::the_pump_loop_releases_the_lock_when_quit_is_raised` (24
admitted jobs stay exactly as admission left them — `MAX_CONCURRENCY` running and
the rest pending, none terminal — so the loop broke before its first pump step),
`lifecycle::tests::every_wake_source_is_inert_once_quit_was_raised` (all four
sources, including `ResumeAfterSuspend`),
`lifecycle::tests::a_resume_detected_after_quit_starts_no_work`,
`lifecycle::tests::the_resume_monitor_stops_itself_once_quit_was_raised` and
`lifecycle::tests::the_resume_monitor_keeps_sampling_until_quit`.

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

Two rounds of smoke runs of `target/debug/quotavista-desktop`, each with a fresh
`mktemp -d` `TMPDIR` and `QUOTAVISTA_SECRET_STORE=session-only`. stderr is quoted
verbatim. Round 1 was observed at commit `4045a2e`; round 2 was re-run after the
review-round-1 fixes, which is why the notification capability reads differently.

### Round 1 (commit `4045a2e`)

Round 1 reported `notification=Available`. That is exactly what review round 1
raised as Blocking 1, and the current code reports `Unconfirmed`; the lines below
are kept as the record of what that commit actually printed.

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
   evidence that the handover reaches the gated wake path and that refresh-Off
   starts nothing on a real reactivation. It is **not** evidence of a
   sleep/resume cycle: that is `ResumeMonitor`, whose real-machine check is
   `scripts/native-checks.sh --interactive` step 14.
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
`ExitRequested` are all main-thread. Review round 1 later removed `block_on` from
every callback anyway (Should-fix 3 below), so this argument now only explains why
the marshalling was not the cause of the panic; the `run_on_main_thread` call stays
because `show_main_window` is a window operation.
Using `tauri::async_runtime::spawn` instead was rejected because the SQLite pools
in `AppState` are bound to the runtime built in `run()`.

### Round 2 (after the review-round-1 fixes)

4. **Default run.** `quotavista: capabilities tray=Available
   notification=Unconfirmed`. The capability now says only what the pinned plugin
   can prove: the channel exists, but neither permission nor delivery could be
   confirmed. The process stayed alive, and the data directory again contained
   only `quotavista-prototype/{journal, quotavista.sqlite, quotavista.sqlite-shm,
   quotavista.sqlite-wal}`.
5. **Second instance against the first instance's data directory.** The second
   launch exited with status 0 and empty stderr while the first kept running, and
   the first instance's stderr gained `quotavista: wake SingleInstanceActivation
   mode Off permitted false merged 0 not-due 0`. No panic, so the
   `run_on_main_thread` marshalling from round 1 still holds, and the handover
   reaches the gated wake path with the shipped refresh-Off default.
6. **No-tray run.** `QUOTAVISTA_TRAY=off` printed `quotavista: capabilities
   tray=Unavailable notification=Unconfirmed`; the process launched and stayed
   alive. `leftover=0` after cleanup in every round-2 run, and every round-2 run
   used a fresh throwaway `TMPDIR`.

A real machine suspend/resume cycle was **not** part of these runs: it needs a
human at the machine, and it is `scripts/native-checks.sh --interactive` steps 14
and 15.

### Round 3 (after the rebase onto P1-09)

7. **Default run.** `quotavista: capabilities tray=Available
   notification=Unconfirmed`, and the process stayed alive. The data directory now
   also contains `journal/refresh-counter.v1`, because P1-09's merged
   `seed_demo_connections` performs an explicit manual refresh at startup; that
   file is the persisted cooldown counter. Everything else is unchanged
   (`quotavista-prototype/{quotavista.sqlite, quotavista.sqlite-shm,
   quotavista.sqlite-wal}`), and no real application data directory was touched.
8. **Second instance against the first instance's data directory.** Exit status 0,
   zero bytes of stderr, first instance still alive, and its stderr gained
   `quotavista: wake SingleInstanceActivation mode Off permitted false merged 0
   not-due 0`. No panic, so the `run_on_main_thread` marshalling survived the
   rebase and the handover still reaches the gated wake path.
9. **No-tray run.** `QUOTAVISTA_TRAY=off` printed `quotavista: capabilities
   tray=Unavailable notification=Unconfirmed`, with the same clean handover.
   `leftover=0` after cleanup; every round-3 run used a fresh throwaway `TMPDIR`
   and `QUOTAVISTA_SECRET_STORE=session-only`.

The rebase did not change any observed behaviour: the only difference between
round 2 and round 3 is P1-09's startup seeding and its counter file.

## Native targets recorded as pending

GUI automation is **not** genuinely available in this environment, so the
interactive steps are recorded as pending rather than passed. Probes against the
running application returned `count windows` = `0`; asking for `window 1` failed
with `-1719` (`无效的索引`); `{frontmost, visible}` was `{false, true}`;
`set frontmost to true` produced no effect and `keystroke "q" using command down`
never reached the process. Process enumeration does work, so Apple Events are
permitted but UI scripting of this application's windows is not.

`./scripts/native-checks.sh --interactive` therefore reports 16 ×
`OBSERVED: PENDING (no interactive terminal; run this step manually)` and never
guesses a pass. The pending steps are: cold launch, in-app capability read,
English notification banner, Simplified-Chinese notification banner, close with a
tray hides, reopen from the tray, explicit quit from the tray, no-tray close
exits, single instance, wake merges refresh, refresh-Off starts nothing, quit
cancels in-flight work, restart keeps committed data, real suspend/resume with
automatic refresh on, real suspend/resume with refresh off, and window
responsiveness during a slow refresh.

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
- **Real sleep/wake.** Tauri 2.12 has no desktop suspend or resume event, so
  `ResumeMonitor` detects the gap itself by comparing a monotonic and a wall clock
  every 30 seconds and treating an advance of at least 120 seconds as a resume.
  The detection mechanism and its refresh-Off gating are unit-tested, but an
  actual machine suspend cannot be observed from this session; it is interactive
  steps 14 and 15, and the operator records the `wake ResumeAfterSuspend …`
  diagnostic the run prints.
- **Notification capability on Linux.** The pinned plugin's desktop
  `permission_state()` always answers granted, so the adapter reports
  `Unconfirmed` rather than `Available` — which is what the round-2 native runs
  printed. `scripts/native-checks.sh --environment` probes
  `org.freedesktop.Notifications.GetCapabilities` over the session bus on Linux so
  a native run records whether a daemon is actually there; promoting the adapter
  to a real detection needs that probe validated on the target, which is P1-13
  work.
- **macOS notification attribution.** In a dev build the plugin attributes
  notifications to `com.apple.Terminal` (`src/desktop.rs:518-524`), so a banner
  observed from `cargo run` is not attributed to QuotaVista. A bundled build is
  needed for that, which is out of scope here.
- **Bilingual tray labels.** The tray is registered once at start-up while locale
  switching lives in the renderer, so localized tray labels would need a
  settings-driven re-registration. The labels stay English and this is recorded
  rather than faked; the notification text *is* localized because it is rendered
  per request.

## Review round 1

An independent review of commit `164218d` returned CHANGES_REQUESTED with two
blocking findings and one should-fix. All three are addressed on this branch. The
reviewer's own reruns of the round-1 tree were green, so nothing below disputes
them.

**Blocking 1 — notification availability was assumed, and unconfirmed delivery was
surfaced as acceptance.** The adapter mapped the plugin's desktop
`permission_state()` to `Available` even though that call unconditionally returns
`Granted`, and mapped `show() == Ok(())` to delivered even though the plugin
spawns the delivery and never reads the operating-system result. An absent Linux
notification service, or notifications disabled in System Settings, could
therefore still produce "Available" and "The platform accepted the notification."

Fix: `NotificationCapabilityDto` gained `Unconfirmed`, which is what the pinned
plugin yields on desktop, and `Available` is now reserved for a channel whose
delivery the platform confirmed. `NotificationPreviewDto.delivered: bool` became
`delivery: NotificationDeliveryDto` with `Delivered`, `Queued` and `Refused`, so
"handed over, result never observed" is a state of its own instead of a `true`.
`PlatformState::preview` maps only a confirmed delivery to `Available`, so a
queued one can never upgrade an unconfirmed channel, and it still returns the
rendered text on every path. Both mappings of the real adapter are extracted into
`capability_from_plugin` and `delivery_from_plugin`, so they are testable without a
window server and without sending anything to the operating system. The renderer
gained `desktop.notification.unconfirmed` and `desktop.notification.queued` in
both catalogs and renders the delivery state through a key map instead of a
boolean.

Proof: `platform::tests::a_plugin_that_answers_granted_is_reported_as_unconfirmed`,
`platform::tests::plugin_success_is_queued_because_the_os_result_is_never_read`,
`platform::tests::a_queued_notification_stays_unconfirmed_and_keeps_its_text` (the
regression the review asked for) and
`platform::tests::only_a_confirmed_delivery_reports_delivered`; in the renderer,
`reports a queued notification as unconfirmed rather than accepted`. Observed
natively: round 2 printed `notification=Unconfirmed` where round 1 printed
`notification=Available`. A real Linux daemon probe is recorded as pending rather
than guessed.

**Blocking 2 — reactivation is not a sleep/wake adapter.** The wake sources were
window focus, the macOS reopen event and the second-instance handover, so a resume
while the main window stayed hidden in the tray emitted none of them and the
permitted post-wake refresh never ran. The review was right that the recorded
single-instance handover is evidence of Off gating, not of a sleep cycle.

Fix: `ResumeMonitor` is a suspend-gap mechanism independent of window activation.
`run()` spawns `drive_resume_monitor` on the composition runtime; it samples a
monotonic and a wall clock every `RESUME_TICK` and treats an advance of at least
`RESUME_THRESHOLD` as a resume, reporting `WakeSource::ResumeAfterSuspend`. Both
clocks are compared and either one advancing is enough, because Rust's `Instant`
is `mach_absolute_time` on macOS and does not advance during sleep, while Linux
`CLOCK_MONOTONIC` behaviour across suspend varies by kernel and the wall clock
always advances. The resume path calls the same `handle_gated_wake` as every other
source, so Off gating and deduplication are preserved by construction instead of
being duplicated.

Proof: `lifecycle::tests::a_suspend_gap_is_detected_from_either_clock`,
`the_first_resume_sample_only_establishes_a_baseline`,
`ordinary_ticks_and_a_backwards_clock_are_not_a_resume`,
`a_detected_resume_merges_one_refresh_without_window_activation` and
`a_detected_resume_starts_nothing_while_refresh_is_off`. A real machine suspend is
still interactive steps 14 and 15.

**Should-fix 3 — the refresh pipeline ran synchronously on the event-loop thread.**
`wake` and `quit` called `runtime.block_on`, so with automatic refresh enabled a
focus event waited for the settings read, the admission writes and the whole
bounded pump loop before the window could respond — including to the user's Quit
action. The pump-step bound caps the number of steps, not the duration of each
one.

Fix: both callbacks now only clone an `AppHandle` and `runtime.spawn`, so the
event-loop thread keeps only window operations. `AppState.shutdown_signal` provides
the coordination the review asked for: `shutdown` raises it before it waits for the
refresh lock, `handle_gated_wake` checks it before the gate so a wake arriving
while quitting neither consumes the gate's slot nor competes for that lock, and
`handle_wake`'s pump loop checks it between steps so it releases the lock instead
of running the remaining steps. `quit` exits the process from inside its own task,
after cancellation and the pool close.

Admission stays coordinated by the same `state.refresh` mutex the refresh command
already uses: a wake and a manual refresh serialize on it, and
`RefreshCoordinator` coalesces a second request for a connection that already has
a live job, so spawning instead of blocking cannot double-admit work.
`the_wake_callback_returns_while_the_refresh_pipeline_is_still_busy` covers the
serialization side — its holder owns that mutex for the whole window — and
`wake_merges_one_refresh_and_a_second_wake_adds_nothing` covers the coalescing
side.

Proof: `tests::the_wake_callback_returns_while_the_refresh_pipeline_is_still_busy`
— the delayed synthetic-work test the review asked for. A holder task owns the
refresh lock for 400ms; the real `wake` callback returns in under 100ms, and
shutdown then completes with nothing running, nothing pending and no event
published. Also
`lifecycle::tests::the_pump_loop_releases_the_lock_when_quit_is_raised`,
`every_wake_source_is_inert_once_quit_was_raised`,
`a_resume_detected_after_quit_starts_no_work`,
`the_resume_monitor_stops_itself_once_quit_was_raised` and
`the_resume_monitor_keeps_sampling_until_quit`.

The single-instance callback keeps its `run_on_main_thread` marshalling from round
1, because the plugin dispatches it from its own async-runtime task; the marshalled
closure now only shows the window and spawns.

**Rebase onto the merged P1-09.** While these fixes were in flight, P1-09 merged
into `main` as `757facf` (archived by `60a4649`) and this branch was rebased onto
it. Both conflicts were additive and were resolved by keeping both sides:
`mod statistics;` next to `mod lifecycle;` and `mod platform;` in
`apps/desktop/src-tauri/src/lib.rs`, and the `## P1-09 prototype evidence` section
next to `## P1-11 prototype evidence` in `docs/traceability.md`. A clean
`git rebase --continue` did not mean a compiling tree: P1-09's
`runtime.block_on(seed_demo_connections(&state))` inside `setup` moved the owned
runtime out from under `app.run`, and a test-module import was missing. Both are
fixed and recorded under Repository validation. No P1-09 logic was rewritten.

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
  usable surface. Review round 1 rewrote it into the vocabulary the fix shipped:
  `Available` is reserved for a channel whose delivery the platform confirmed,
  `Unconfirmed` means the channel exists but nothing could be verified, and
  `NotificationPreviewDto.delivery` separates `Delivered`, `Queued` and
  `Refused` so only a confirmed delivery may upgrade the capability.
- `scripts/native-checks.sh` — new, executable, `bash -n` clean. `--environment`
  records the host and, on Linux, probes `org.freedesktop.Notifications` and the
  StatusNotifierWatcher host so a native run records a detection instead of
  trusting the plugin's always-granted answer. `--automated` runs three labelled
  test steps from their real exit status. `--interactive` walks 16 observed steps
  and prints `OBSERVED: PENDING` rather than a guessed pass when there is no
  terminal; steps 14 to 16 are the real suspend/resume and responsiveness checks
  review round 1 asked for.
- `docs/traceability.md` — the A07, A11 and A25 bullets of the P1-11 section were
  rewritten after review round 1 to name both wake sources, the three delivery
  states and the post-rebase catalog size.
- No other spec, requirement or architecture document was changed, and no sibling
  task directory was touched.

## Continuous integration

Pull request #11 (`jondong/p1-11-desktop-lifecycle` → `main`). Every push was
watched with `gh pr checks 11 --watch` or `gh run list`, and both required checks
completed successfully on every head this branch has had.

| Head | Content | `workspace-checks` | `macos-desktop` | Run |
| --- | --- | --- | --- | --- |
| `4045a2e` | round-1 implementation | success, 1m25s | success, 3m22s | 39541645343 |
| `164218d` | round-1 validation record | success, 2m13s | success, 4m25s | 39542209346 |
| `2668cc5` | review fixes and the two rebase repairs | success, 1m38s | success, 3m23s | 39556096304 |
| `f358fd0` | review-round and rebase records | success, 2m4s | success, 4m10s | 39556511256 |
| `ab208d1` | execution-plan checkboxes, count corrections | success, 1m34s | success, 4m31s | 39556945222 |

`4045a2e` and `164218d` predate the rebase onto P1-09, so those two SHAs are no
longer on this branch; their observed results are kept because they are what
actually ran. `2668cc5` is the last head that changed any code — every head after
it is markdown only, and each still ran the full pipeline. The final row names the
head this row was committed on top of, because a commit cannot contain its own
SHA; the follow-up documentation commit's run is green as well and is visible in
the pull request's check history.

Three of the watchers died on transient GitHub API errors
(`Post "https://api.github.com/graphql": unexpected EOF`), so the results above
were re-read directly from `gh run view` per run rather than taken from a
watcher's last line.

The ubuntu job excludes `quotavista-desktop` from Clippy and tests, so the two new
desktop-only plugin dependencies are compiled only by the macOS job; that job runs
the full workspace, including the 62 desktop tests, and the unsigned debug build.
Per `.trellis/spec/backend/quality-guidelines.md` and the workflow's own comment, a
green CI build is supplementary: it does not close G01, G02 or G04 and is not
evidence of a native Linux or macOS run. The native observations above and the
pending list below carry that evidence.

## Open items

- All 16 interactive native steps above, on macOS and on Ubuntu 26.04 GNOME.
- Linux tray detection stays conservatively `Unavailable` until a host probe is
  validated by a native run (P1-13).
- `delivery: Queued` is the most the pinned desktop plugin can honestly report; a
  real delivery receipt would need the plugin's tracked-notification path plus a
  listener permission, which is out of scope for a prototype that must not widen
  the renderer's capability allowlist.
- Bilingual tray labels need settings-driven re-registration.
- P1-09 is merged into `main` (`757facf`, archived by `60a4649`) and this branch
  was rebased onto it. Both conflicts were additive and were resolved by keeping
  both sides: `mod statistics;` next to `mod lifecycle;`/`mod platform;` in
  `apps/desktop/src-tauri/src/lib.rs`, and the `## P1-09 prototype evidence`
  section next to `## P1-11 prototype evidence` in `docs/traceability.md`. P1-10
  still owns the connections and settings screens, so this branch stays additive in
  the shared files (`lib.rs`, `ipc.ts`, `contracts`, catalogs, `Cargo.lock`,
  `traceability.md`) to keep that merge independent. `Cargo.lock` was resolved with
  `cargo fetch` rather than `cargo generate-lockfile` to avoid bumping six
  unrelated packages.
