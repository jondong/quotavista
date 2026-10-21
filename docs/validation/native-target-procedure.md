# Native target procedure

Reproducible instructions for collecting native desktop evidence for the Phase 1
prototype, plus the synthetic setup and reset steps every run depends on. This
document is the checklist an operator follows on a real desktop; the results are
recorded in a task `validation.md` and summarised in
[phase-one-exit-assessment.md](phase-one-exit-assessment.md).

Two targets are in scope for Phase 1:

| Target | Requirement |
| --- | --- |
| macOS | The recorded development host (Apple silicon). The minimum supported release stays open under G04. |
| Linux | Ubuntu 26.04 LTS, GNOME, x86_64, in a real desktop session (Wayland or X11 — record which). |

Nothing in this document may be satisfied by a CI run. `.github/workflows/ci.yml`
says so itself: compilation on a hosted runner is not evidence of a native run.

## What counts as evidence

A row is evidence only when it names all five of:

1. the commit (`git rev-parse HEAD`) the run used,
2. the host (OS name and version, build, architecture; on Linux also
   `XDG_SESSION_TYPE`, `XDG_CURRENT_DESKTOP`, and the GNOME Shell version),
3. the exact command or the exact user action,
4. the actual observation — stdout/stderr text, an exit status, a database row,
   or what the operator saw on screen,
5. the limitation, including anything the run could not observe.

A successful build is never desktop usability. A step that was not run is
recorded as `PENDING` or `UNVERIFIED`, never as passed, and never inferred from
another platform.

## Safety rules (non-negotiable)

- Synthetic data only. No provider account, no real credential, no production or
  business database, not even for reads.
- Never use or touch the real application data directory. The prototype stores
  its state in `${TMPDIR:-/tmp}/quotavista-prototype`; every scripted run in this
  procedure overrides `TMPDIR` to a fresh throwaway directory so the default
  location is never created, read, modified, or deleted.
- Never delete a real application data directory as a rollback shortcut.
- Any real keychain or Secret Service access must use disposable entries, must be
  cleaned up by the test itself, and must be followed by an independent check
  that the entries are gone. The disposable **service** name starts with
  `quotavista-test-` — the smoke tests build
  `quotavista-test-<task>-<pid>` (`crates/infrastructure/src/secrets/tests.rs`) —
  while the **account** names are the application's normal reference keys,
  `qv1:macos-keychain:synthetic-entry-a` and `-b`. The prefix rule therefore
  applies to the service, which is what selects the entry group; the cleanup
  check searches that service and also greps the whole keychain for
  `quotavista-test`. The application's own production service name is `quotavista`
  (`crates/infrastructure/src/secrets.rs`), which does not carry that prefix, so
  the prototype is never launched with the native store during validation.
- Launch the prototype with `QUOTAVISTA_SECRET_STORE=session-only` for every
  scripted native run. That is the documented, visible opt-in to the in-memory
  store (`apps/desktop/src-tauri/src/lib.rs`, `secret_choice_from_env`), so no
  access prompt can block an unattended run and no credential reaches disk.
- Do not attempt to observe a "locked" native store by locking the operator's own
  login keychain or keyring. Locked and unavailable stores are covered by unit
  tests; native observation of them is optional and must not disrupt the session.

## Prerequisites

Install per [CONTRIBUTING.md](../../CONTRIBUTING.md):

- Rust `1.99.1` (from `rust-toolchain.toml`, rustup installs `rustfmt` and
  `clippy` with it), Node.js `24` (from `.nvmrc`), npm `11.19.1` (from
  `apps/desktop/package.json` `packageManager`).
- macOS: Xcode Command Line Tools (`xcode-select --install`).
- Ubuntu/Debian: `sudo apt install libwebkit2gtk-4.1-dev build-essential curl
  wget file libxdo-dev libssl-dev libayatana-appindicator3-dev
  librsvg2-dev`. Add `libdbus-1-dev` and `sqlite3` (the probe reads the
  synthetic store back with the system `sqlite3`, and the Secret Service client
  library is needed to build).

Then, from the repository root:

```sh
npm install -g "npm@$(node -p "require('./apps/desktop/package.json').packageManager.split('@')[1]")"
(cd apps/desktop && npm ci)
cargo fetch
cargo build --locked -p quotavista-desktop
```

The non-interactive probe additionally builds a **validation-only** copy of the
same binary with a different application identifier:

```sh
TAURI_CONFIG='{"identifier":"com.quotavista.prototype.nativeprobe"}' \
    CARGO_TARGET_DIR=target/native-probe cargo build --locked -p quotavista-desktop
```

`scripts/native-launch-probe.sh` runs that build itself (step 0b), so this is only
needed for a manual run. See "Single-instance isolation" below for why.

`cargo build` is required before any native launch: the scripts run the built
binary at `target/debug/quotavista-desktop` in the repository root, and bundling
is disabled in `apps/desktop/src-tauri/tauri.conf.json`, so there is no
`.app`/`.deb` to install. Run the prototype from a terminal so its stderr
diagnostics are captured — they are the machine-readable half of the evidence.

## Synthetic setup and reset

Setup needs no configuration file and no account. The prototype seeds two
synthetic demo connections on first launch and runs one refresh against
synthetic providers (`seed_demo_connections` in
`apps/desktop/src-tauri/src/lib.rs`). A store that already holds connections is
left untouched, so a second launch on the same directory is a warm start, not a
reseed.

Reset between runs:

```sh
# Scripted runs: delete only the throwaway probe directory the script created.
rm -rf "${TMPDIR:-/tmp}"/qv-native-probe.*

# Manual runs: export the ONE mandatory session block before launching anything.
# Both variables are required; a block that exports only TMPDIR still leaves the
# prototype on the operator's real Keychain or Secret Service.
export QUOTAVISTA_NATIVE_TMPDIR="$(mktemp -d "${TMPDIR:-/tmp}/qv-native-session.XXXXXX")"
export TMPDIR="$QUOTAVISTA_NATIVE_TMPDIR"
export QUOTAVISTA_SECRET_STORE=session-only
```

Never reset by deleting `${TMPDIR:-/tmp}/quotavista-prototype` unless you created
it yourself during this validation; on a shared or daily-driver machine that
path may hold another run's state. Prefer a fresh `TMPDIR` per attempt.

`scripts/native-checks.sh --interactive` prints this same block, plus launch
commands built by `qv_launch_command`, which cannot drop either export.

### Single-instance isolation

Overriding `TMPDIR` configures the process you start. It does **not** reconfigure
an instance that is already running, and it does not move the single-instance
channel. `tauri-plugin-single-instance` 2.5.4 derives that channel from the
application identifier alone:

- macOS — `PathBuf::from(format!("/tmp/{}_si.sock", identifier))` with `.` and `-`
  replaced by `_`, so `com.quotavista.prototype` always uses
  `/tmp/com_quotavista_prototype_si.sock` (`src/platform_impl/macos.rs:61`).
  `TMPDIR` is never consulted.
- Linux — the session D-Bus name `<identifier>.SingleInstance`
  (`src/platform_impl/linux.rs:31`), settable only from Rust via `Builder::dbus_id`.

A second launch connects to that channel first and, if anything is listening,
exits 0 after handing the activation over — so it wakes whatever is already
running, showing its window and running its wake path against its own state. A
stale socket is left behind when a process is terminated by a signal, because the
plugin removes it only on `RunEvent::Exit`.

This procedure therefore runs the non-interactive probe under a **validation-only
identifier**, `com.quotavista.prototype.nativeprobe`, built by `TAURI_CONFIG` into
a separate `CARGO_TARGET_DIR=target/native-probe`. That gives the run a disjoint
channel and cannot activate an ordinary instance. The probe also:

- fails closed before launching if either socket has a live holder or either
  binary is already running (`qv_require_clear_host`), and refuses to run at all
  when `lsof` is unavailable, because it could then not tell a live socket from a
  stale one;
- records the ordinary socket's modification time before and during the run, so
  the transcript shows it was never bound or activated (step 1b);
- removes only the validation socket on cleanup, never the ordinary one.

Manual runs must respect the same boundary: use the validation identity, or accept
explicitly that a second launch activates whatever is already up on this machine.

To verify what a run committed, read the store back read-only (it never contains
credential values — see `crates/infrastructure/migrations/0001_persistence.sql`,
"No credential values or raw provider payloads"):

```sh
db="${TMPDIR:-/tmp}/quotavista-prototype/quotavista.sqlite"
sqlite3 "file:${db}?mode=ro" \
  'SELECT s.data_revision,
          (SELECT COUNT(*) FROM connections),
          (SELECT COUNT(*) FROM provider_scopes),
          (SELECT COUNT(*) FROM usage_facts),
          (SELECT COUNT(*) FROM published_partitions),
          (SELECT COUNT(*) FROM sync_runs),
          (SELECT t.locale || "/" || t.reporting_zone || "/" || t.currency FROM settings t)
     FROM store_state s;'
```

**Counts alone are not proof that the fixture initialised.** A store can answer
every one of those `COUNT(*)` subqueries and still be an empty schema, or a
half-seeded one, and a check that only compares "cold equals warm" accepts any
identical pair — including `0|0|0|0|0|0|zh-CN/UTC/USD`. The reviewer's
reproduction of exactly that false pass is why
`scripts/test-native-launch-probe.sh` exists. A valid first-launch fixture on any
target is:

| Field | Expected | Where the value comes from |
| --- | --- | --- |
| `store_state.data_revision` | `8` | each committed write advances it by one (`sqlite.rs`, `advance_data_revision`) |
| connections | `2`, ids exactly `synthetic-kimi-demo` and `synthetic-openrouter-demo` | `synthetic_providers()` in `crates/providers/src/lib.rs` |
| connection lifecycle | `active` on the cold run; `needs_credentials` on every later launch | `reconcile_connection_credentials`, `crates/infrastructure/src/secrets.rs` |
| `credential_ref` non-null | `0` | seeding writes no secret |
| provider_scopes | `2`, key `synthetic-account`, type `account`, no orphans | `seed_demo_connections` |
| usage_facts | `6` | three settled UTC days per connection (`HISTORY_DAYS = 3`) |
| published_partitions / sync_runs | `2` / `2` | the seeded refresh published one partition per scope |
| settings | `en/UTC/USD` | the migration's seed row |

`scripts/native-launch-probe.sh` asserts all of these per launch and fails the run
on any mismatch, and it fails on the stderr diagnostics `demo seeding failed` and
`credential store unavailable` instead of merely printing them.

## Environment capture

Run first, on every target, and paste the whole block into the evidence record:

```sh
./scripts/native-checks.sh --environment
```

It prints date (UTC), kernel, distribution or macOS product/build, architecture,
session type, desktop, GNOME Shell version when present, the Rust/Cargo/Node/npm
versions, the locked Tauri version, and the prototype data directory. On Linux it
also probes the session bus twice with `gdbus`:

- `org.kde.StatusNotifierWatcher` `IsStatusNotifierHostRegistered` — whether a
  tray host is actually running,
- `org.freedesktop.Notifications` `GetCapabilities` — whether a notification
  daemon is reachable.

Those two probes are the host facts the capability adapters cannot infer by
themselves; record them verbatim.

## Automated native checks

```sh
./scripts/native-checks.sh --automated      # lifecycle, platform adapters, sealed IPC
./scripts/test-native-launch-probe.sh       # regression suite for the two scripts below
./scripts/native-launch-probe.sh            # real launches, see below
```

`--automated` runs three `cargo test` filters against the desktop crate and
prints `RESULT: PASS`/`RESULT: FAIL` from each real exit status, ending in
`AUTOMATED: PASS` or `AUTOMATED: FAIL`.

`scripts/native-launch-probe.sh` is the non-interactive part of the native run.
It launches the built binary up to three times, each in its own throwaway
`TMPDIR` with `QUOTAVISTA_SECRET_STORE=session-only`, waits for the
`quotavista: capabilities` startup line, and records:

| Step | What it proves |
| --- | --- |
| 0a incumbent gate | Nothing already holds either single-instance socket and neither binary is running, checked without connecting to the socket. The probe **refuses to run** (`PROBE: REFUSED`, exit 2) when `lsof` is unavailable, because it could then not distinguish a live socket from a stale one. |
| 0b isolated build | The validation-only binary exists and actually carries the identifier `com.quotavista.prototype.nativeprobe`. |
| 1 cold launch | The binary starts, the composition initialises, and the tray/notification adapters report their detections. Time from `exec` to the capabilities line is measured, so a launch that never arrived cannot look fast. Reaching that line is a startup milestone of the process, not evidence that a usable window was rendered. |
| 1b isolation held | The ordinary socket's modification time is unchanged across the run and the validation socket appeared, so no launch bound or activated the ordinary channel. |
| 2 single instance | A second launch exits early while the first keeps running and logs the handover wake. Process-level only. |
| 3–4 terminate and validate cold | The process stops on `SIGTERM` with a real exit status, and the committed synthetic state is validated field by field against the fixture table above — not merely read back. |
| 5–7 warm launch | The same directory restarts, does not reseed, keeps the identical aggregate, and shows both demo connections at `needs_credentials` as the startup credential reconciliation requires. |
| 8 no-tray launch | `QUOTAVISTA_TRAY=off` is honoured: the adapter reports `tray=Unavailable`. This exercises the forced-unavailable branch of the override; it is not the discovery of a genuinely missing tray. |
| 9 | An explicit list of what the probe did **not** observe. |

Every capability the probe cannot see is printed as `NOT OBSERVED: …` rather than
as a pass. Add `--keep` to leave the launch directories in place for inspection,
and `--skip-build` to reuse an existing validation binary.

`scripts/test-native-launch-probe.sh` is the regression suite for both scripts. It
launches nothing and touches no credential store; it builds SQLite fixtures on the
real migrations and asserts that the validator accepts a known-good store and
rejects a schema-only store, a seed without committed facts, a seed without
published partitions, wrong connection identities, an orphan scope, a persisted
credential reference, an off-fixture scope key, a wrong lifecycle, a wrong data
revision, and wrong settings. It also asserts the single-instance isolation logic
against a synthetic incumbent, a stale socket, and the three spellings of a binary
path that `ps` can show. Run it after changing either script.


Two manually gated checks complete the automated set:

```sh
QUOTAVISTA_BENCHMARK=1 cargo test --locked -p quotavista-desktop \
  statistics_query_mix_benchmark -- --nocapture
cargo test --locked -p quotavista-infrastructure \
  macos_keychain_synthetic_roundtrip -- --ignored --nocapture   # macOS
cargo test --locked -p quotavista-infrastructure \
  linux_secret_service_synthetic_roundtrip -- --ignored --nocapture   # Linux
```

The benchmark writes `apps/desktop/src-tauri/target/p1-12-benchmark.json`
(gitignored; override the path with `QUOTAVISTA_BENCHMARK_OUT`). The artifact
records `machine: null` and `toolchain: null`, so the evidence record must name
the host itself. The secret-store smoke tests are `#[ignore]`d because they touch
the host store; each uses a disposable `quotavista-test-` service, cleans up, and
prints its own cleanup evidence. The Linux twin records itself pending instead of
failing when no session bus is present.

After the keychain or Secret Service smoke test, verify independently that the
disposable entries are gone:

```sh
security dump-keychain | grep -c 'quotavista-test'          # macOS: expect 0
```

On Linux, the equivalent check is
`secret-tool search service quotavista-test-<pid>` returning no items, or
`busctl --user` inspection of the collection; record whichever the host provides.

## Interactive native checks

```sh
./scripts/native-checks.sh --interactive
```

Before the first step, export the mandatory session block printed at the top of
the transcript — `QUOTAVISTA_NATIVE_TMPDIR`, `TMPDIR`, and
`QUOTAVISTA_SECRET_STORE=session-only` — in the shell that will launch the app,
and launch with one of the printed `env …` commands rather than a bare
`cargo run`. A bare `cargo run` in a clean shell selects the platform secret
store, whose real service name is `quotavista`, and uses the ordinary
single-instance channel. The transcript prints both the validation-identity and
ordinary-identity commands so the choice is explicit rather than accidental.

Sixteen steps need a human at the desktop: cold launch, in-app capability read,
English and Simplified-Chinese synthetic notification banners, close with a tray,
reopen from the tray, explicit quit from the tray, no-tray close exits, single
instance focus, wake merging refresh work, refresh-Off starting nothing, quit
cancelling in-flight work, restart keeping committed data, real suspend/resume
with automatic refresh on and off, and window responsiveness during a slow
refresh.

The script prints each step with what to look for and prompts
`OBSERVED (type what happened, or "skip"):`. With no interactive terminal it
prints `OBSERVED: PENDING (no interactive terminal; run this step manually)` and
never guesses a result — so an unattended run produces sixteen honest `PENDING`
rows. Run it from a real terminal on the target desktop and type the actual
observation for each step.

Record for each step: the step number, the operator's typed observation, and the
matching stderr diagnostics. The startup line to expect is
`quotavista: capabilities tray=<…> notification=<…>`; wake lines look like
`quotavista: wake <source> mode <…> permitted <…> merged <n> not-due <n>`; quit
cancellation prints `quotavista: quit cancelled <n> in-flight job(s)`.

Platform notes that change what a correct observation looks like:

- **Linux tray.** `detect_tray` in `apps/desktop/src-tauri/src/platform.rs`
  reports `Unavailable` on Linux even after a successful registration, because
  `tray-icon`'s libappindicator backend registers fine with no indicator host
  running, so registration is not proof of a visible icon. The no-tray close
  policy is therefore the Linux default: closing the main window exits. Use
  `QUOTAVISTA_TRAY=off` when the same no-tray procedure has to be reproduced on
  macOS. Promoting Linux to `Available` needs a host probe validated by a native
  run and is not done yet.
- **Notifications.** The pinned plugin's desktop `permission_state` always
  answers granted without asking the operating system, so the adapter reports
  `Unconfirmed` rather than `Available`, and a successful `show` maps to `Queued`
  — "the plugin accepted the notice", never "the OS displayed it". A queued
  verdict next to a visible banner is the expected desktop result, not a failure.
  Record both the banner and the in-app verdict.
- **macOS notification attribution.** In a dev build the plugin attributes
  notifications to the launching terminal rather than to QuotaVista, so a banner
  from `cargo run` is not branded. Attributed notifications need a bundled build,
  which is release work under G04.
- **Tray labels stay English.** The tray registers once at start-up while locale
  switching lives in the renderer, so localised tray labels would need
  settings-driven re-registration. Notification text is localised because it is
  rendered per request.
- **Suspend/resume.** Tauri 2 exposes no desktop suspend/resume event, so
  `ResumeMonitor` compares monotonic against wall-clock time and treats an
  advance of at least `lifecycle::RESUME_THRESHOLD` (two minutes) as a resume.
  The real-suspend steps must therefore sleep the machine for longer than two
  minutes.

## Evidence record format

For each target, produce a table with one row per check and columns: check,
command or action, observation, result (`PASS` / `FAIL` / `PENDING` /
`UNVERIFIED`), limitation. Prefix the table with the environment block, the
commit, and the branch. Close with an explicit list of what was not collected and
why, naming the owning follow-up for each item.
