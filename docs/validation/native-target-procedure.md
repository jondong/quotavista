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
- Any real keychain or Secret Service access must use disposable entries whose
  service and account names start with `quotavista-test-`, must be cleaned up by
  the test itself, and must be followed by an independent check that the entries
  are gone. The application's own production service name is `quotavista`
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

# Manual runs: use a fresh directory instead of deleting anything.
export TMPDIR="$(mktemp -d /tmp/qv-manual.XXXXXX)"
```

Never reset by deleting `${TMPDIR:-/tmp}/quotavista-prototype` unless you created
it yourself during this validation; on a shared or daily-driver machine that
path may hold another run's state. Prefer a fresh `TMPDIR` per attempt.

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
| 1 cold launch | The binary starts, the composition initialises, and the tray/notification adapters report their detections. Time from `exec` to the capabilities line is measured, so a launch that never arrived cannot look fast. |
| 2 single instance | A second launch exits immediately while the first keeps running and logs the handover wake. |
| 3–4 terminate and inspect cold | The process stops on `SIGTERM` with a real exit status, and the committed synthetic state is read back from SQLite. |
| 5–7 warm launch | The same directory restarts faster, does not reseed, and keeps the identical data revision and settings. |
| 8 no-tray launch | `QUOTAVISTA_TRAY=off` is honoured: the adapter reports `tray=Unavailable`. |
| 9 | An explicit list of what the probe did **not** observe. |

Every capability the probe cannot see is printed as `NOT OBSERVED: …` rather than
as a pass. Add `--keep` to leave the launch directories in place for inspection.

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
