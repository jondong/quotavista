# P1-13 validation record

## Scope and prerequisites

Branch `jondong/p1-13-native-validation`, 2026-10-21, based on `main` at
`1761be7`. Prototype revision under test: **`1761be7`** — the merged P1-12 head.
The complete check battery was re-run at this task's own commit **`1afa2a8`** and
is recorded under `### Confirming battery at the committed revision` below.
P1-13 changes no file under `apps/` or `crates/`; it adds
`scripts/native-launch-probe.sh`, `docs/validation/native-target-procedure.md`,
`docs/validation/phase-one-exit-assessment.md`, and documentation-status edits.
Every observation below therefore describes the same prototype P1-12 validated.

Host (the only target that was available):

```text
Machine          Mac18,5, Apple M6, 32 GB (34359738368 bytes), 631 GB free
Operating system macOS 27.0.1, build 26A434 (Darwin 27.0.0)
Architecture     arm64
Session type     n/a (Aqua) — macOS has no XDG session type to record
Rust             rustc 1.99.1 (58cf8f9c3 2026-10-13)
Cargo            cargo 1.99.1 (5f94df478 2026-08-27)
Node / npm       v24.22.0 / 11.19.1
Tauri (locked)   2.12.4
Host sqlite3     3.54.0 (used read-only to inspect prototype state)
```

Host-independent toolchains are pinned by `rust-toolchain.toml` (1.99.1),
`.nvmrc` (24), and `packageManager` (`npm@11.19.1`), so an operator on another
machine reproduces the same versions.

Second required target: **Ubuntu 26.04 LTS, GNOME, x86_64 — not available.** No
physical machine, no VM, and no container was provisioned in this session. A
container would not have satisfied the requirement in any case: the target is a
GNOME desktop session with a window server, an indicator host, and a
notification daemon, and a headless build is explicitly insufficient. Every
Linux row in this record and in the exit assessment is `UNVERIFIED`, and no
Linux result was inferred from macOS behaviour or from CI.

Safety boundary actually observed:

- No production or business database was contacted, not even for reads. The only
  providers in the build are the synthetic ones; no network provider call was
  made by any check.
- No real application data directory was touched. The prototype's default state
  directory `${TMPDIR:-/tmp}/quotavista-prototype` did not exist before the
  launch probe and did not exist after it, because every scripted launch
  overrides `TMPDIR` with a throwaway `mktemp -d` directory that the probe
  deletes on exit.
- Real macOS Keychain access came in two kinds, and they are not the same kind of
  access. **Writes and deletes** touched only the disposable service
  `quotavista-test-p1-05-35988` used by the `#[ignore]`d smoke test, under the
  application's normal reference accounts
  `qv1:macos-keychain:synthetic-entry-a` and `-b`; cleanup was verified
  independently of the test (output below). **Read-only diagnostic queries** were
  broader: the cleanup transcript runs `security dump-keychain` (which enumerates
  the whole keychain) and `security find-generic-password -s quotavista` (a
  lookup against the production service name). Neither reads a secret value —
  `find-generic-password` without `-w` prints metadata only, and the query
  returned "could not be found" — but "the entries are absent afterwards" is
  evidence of cleanup, not proof that nothing else was ever enumerated. The
  production service name is `quotavista` (`crates/infrastructure/src/secrets.rs:37`),
  which does not satisfy the `quotavista-test-` rule, so the prototype was never
  launched against the native store: every native launch set
  `QUOTAVISTA_SECRET_STORE=session-only`.
- All persisted data inspected was synthetic demo data seeded by the prototype
  itself.

## Design decisions recorded during implementation

1. **Non-interactive evidence is separated from interactive evidence by
   construction.** `scripts/native-launch-probe.sh` asserts only what a headless
   caller can actually prove — the binary started, the composition initialised,
   the secret-store choice took effect, the platform adapters reported their
   detections, a second launch handed over to the first, and committed synthetic
   data survived a restart — and prints an explicit `NOT OBSERVED: …` line for
   everything else, naming the interactive step that covers it. It has no path
   that prints a pass for a window, a tray icon, or a notification banner.
2. **`SIGTERM` is not the tray Quit path.** The probe's stop step sends
   `SIGTERM` and reports the real exit status (143). Both the script and this
   record state that this is not evidence for interactive step 7 (explicit quit
   from the tray), so a green probe cannot be misread as a quit-path result.
3. **`--interactive` was run without a terminal on purpose.** The transcript
   records 16 × `OBSERVED: PENDING (no interactive terminal; run this step
   manually)` and zero passes, which is the honest degradation: the script asks
   an operator and never guesses.
4. **The locked-store row stays automated-only.** Observing a locked native
   store natively would mean locking the operator's own login keychain, which
   risks unrelated work and proves nothing the unit tests do not. The row is
   marked `PARTIAL` with the automated tests named, not `PASS`.
5. **Linux tray availability was not promoted.** `detect_tray` reports
   `Unavailable` on Linux because libappindicator registration succeeds with no
   indicator host, so registration is not proof of a visible icon. Promoting it
   needs a host probe validated by a native Ubuntu run; changing it without that
   run would be an unvalidated behaviour change in the unsafe direction. It is
   recorded as a Phase 2 entry condition with the `gdbus` probe that would
   supply the evidence.
6. **No `docs/research/` directory was created.** The OS-availability research
   lives in the exit assessment's `## Environment availability at assessment
   time` section with its sources, so there is one place for those facts instead
   of two.
7. **Links broken by the P1-12 archiving were repaired here** rather than left
   for a later task, because A24 counts valid local links as evidence and this
   task was already editing those documents.
8. **Single-instance isolation comes from the application identifier, not from
   `TMPDIR`.** An independent review reproduced cross-directory activation: the
   pinned `tauri-plugin-single-instance` 2.5.4 derives its macOS channel as
   `format!("/tmp/{}_si.sock", identifier)` with `.` and `-` replaced by `_`
   (`src/platform_impl/macos.rs:61`), never consulting `TMPDIR`, and on Linux uses
   the session D-Bus name `<identifier>.SingleInstance`
   (`src/platform_impl/linux.rs:31`). A fresh `TMPDIR` and
   `QUOTAVISTA_SECRET_STORE=session-only` configure the process being started; they
   do not reconfigure an incumbent, which can still receive the activation, show
   its window and run its wake path against its own state. The probe now builds a
   validation-only binary with `TAURI_CONFIG='{"identifier":"com.quotavista.prototype.nativeprobe"}'`
   into `CARGO_TARGET_DIR=target/native-probe`, giving it the disjoint channel
   `/tmp/com_quotavista_prototype_nativeprobe_si.sock`; step 1b records that the
   ordinary socket's recorded stamp is unchanged across the run — a point-in-time
   observation that supports "no recorded change", not "no activation", since
   connecting to an existing socket need not change its mtime. Detection inspects
   the channel that actually exists on the platform — `lsof` holders of the Unix
   socket on macOS, `org.freedesktop.DBus.NameHasOwner` for
   `<identifier>.SingleInstance` on Linux — plus `ps` argv matching, and it never
   connects to the socket, because connecting *is* the activation being avoided.
   A channel state that cannot be interpreted is reported as an incumbent rather
   than as clear. A loose `pgrep -f quotavista` was rejected after it matched the
   agent process itself, whose environment merely contains the worktree path.
9. **The probe fails closed on an incumbent, on a missing inspection tool, and on
   an unverifiable binary.** Before any launch, `qv_require_clear_host` requires
   that neither channel has a live holder and that neither binary is running; when
   the platform's inspection tool is absent — `lsof` on macOS, `gdbus` on Linux —
   the probe exits 2 with `PROBE: REFUSED` rather than proceeding, because it
   could then not distinguish an occupied channel from a free one. It also takes
   `mkdir /tmp/quotavista-native-probe.lock`, released on every exit path
   including the refusals: the validation identifier is a fixed constant, so two
   overlapping runs would share one channel, and a pre-flight check is not a lock.
   The identifier is read back out of the binary before launching, so
   `--skip-build` cannot reuse a cached artifact built for the ordinary identity —
   a post-launch identity check could not undo an activation. Cleanup removes only
   the validation socket, and only when it is demonstrably stale rather than held;
   the ordinary socket is never touched, even when it is stale.
10. **A store inspection that only compares aggregates accepts an empty store.**
    The review demonstrated `PROBE: PASS` against a disposable test double that
    read back `0|0|0|0|0|0|zh-CN/UTC/USD`, because "cold equals warm and non-empty"
    was the whole criterion. The probe now validates the fixture field by field
    against the expectations in `docs/validation/native-target-procedure.md`
    (revision 8, the two `*-demo` connection identities, scope key/type, no
    orphan scopes, no non-null `credential_ref`, 6 facts, 2 published partitions,
    2 sync runs, `en/UTC/USD`), treats a readable store and a *valid* store as
    separate judgements, and fails on the `demo seeding failed` and
    `credential store unavailable` diagnostics instead of only printing them.
    `scripts/test-native-launch-probe.sh` carries the store fixtures, including the
   reviewer's exact false-pass shape. Counts alone do not establish that the
   fixture is the fixture, so the validator also checks relationships and content:
   scope identities, facts joined to a partition of a *different* scope, published
   partitions pointing at another partition's run, facts and partitions per scope,
   fact kinds, and that each scope's amounts are distinct, numeric, positive and
   contiguous (which is the shape `simulated::history_facts` writes). Six
   count-preserving corruptions in the self-test keep the aggregate row at
   `8|2|2|6|2|2|en/UTC/USD` and are still rejected.
11. **The warm launch's `needs_credentials` lifecycle is expected, and naming it
    was the point.** The first validator run rejected the warm store with
    `found 'needs_credentials ', expected 'active '`. That is not a regression:
    `reconcile_connection_credentials` (`crates/infrastructure/src/secrets.rs:47`)
    marks an `Active` connection whose `credential_ref` is absent or unreadable as
    `NeedsCredentials`, which is exactly what `lib.rs` promises — "a session-only
    credential never survives a restart, so its connection reports
    `needs_credentials` in the first list the renderer reads". The expectation is
    therefore per-launch (`EXPECTED_LIFECYCLES_SEEDED` vs
    `EXPECTED_LIFECYCLES_RESTARTED`) rather than a constant, and this is the first
    native observation of that promise rather than only a unit test of it.
12. **A command an operator is told to copy has to be executed by a test.** The
    documented manual-session block once printed
    `export QUOTAVISTA_NATIVE_TMPDIR="$(mktemp -d "${TMPDIR:-/tmp/qv-native-session.XXXXXX")}"`
    — a mis-nested brace that made `mktemp` create nothing usable, while a
    substring assertion on `export QUOTAVISTA_NATIVE_TMPDIR=` still passed. The
    self-test now runs the printed block in a real `bash -c` and requires a
    directory, `TMPDIR` repointed at it, and `QUOTAVISTA_SECRET_STORE=session-only`.

## PRD criterion evidence

### Criterion 1 — Native evidence includes actual observations for each target; Linux is not marked passed from CI or macOS behavior.

**NOT MET — one of two targets carries observations.** Left unchecked in the PRD.

macOS actual observations, from `scripts/native-launch-probe.sh` (exit 0,
`PROBE: PASS`) inside the confirming battery at the committed revision
`1afa2a8`, each launch in its own throwaway `TMPDIR` under
`QUOTAVISTA_SECRET_STORE=session-only`. An identical run at the pre-commit base
`1761be7` produced the same capability lines and the same store row, which is
what "this task changes no application source" predicts.

- Cold launch (pid 33180): **1,174 ms** from `exec` to
  `quotavista: capabilities tray=Available notification=Unconfirmed`; resident
  set 123,424 KiB; state directory contained `journal/`,
  `journal/refresh-counter.v1`, `quotavista.sqlite`, `-shm`, `-wal`; the
  capabilities line was the only `quotavista:` diagnostic, so no seeding
  failure, no degraded credential store, and no tray registration error.
- Single instance: a second launch (pid 33369) **exited 0 after 132 ms** while
  the first kept running and logged
  `quotavista: wake SingleInstanceActivation mode Off permitted false merged 0 not-due 0`.
  This is real evidence for both the handover and for Off-mode wake gating.
- Warm launch (pid 33518) on the same state directory: **454 ms**, same
  capabilities line, no reseed.
- Store inspection, read-only, cold and warm both
  `data_revision 8 | connections 2 | provider_scopes 2 | usage_facts 6 |
  published_partitions 2 | sync_runs 2 | settings en/UTC/USD` — identical, so
  committed data survived the restart and seeding did not repeat.
- No-tray mode (pid 33738, `QUOTAVISTA_TRAY=off`): **445 ms**,
  `quotavista: capabilities tray=Unavailable notification=Unconfirmed`.
- Native Keychain round-trip on the real host store (see criterion 2).

#### Launch observations from the rewritten probe

The bullets above came from the probe as it stood at `1afa2a8`. An independent
review then found two defects in that probe (B1: `TMPDIR` does not isolate the
single-instance channel; B2: the store inspection accepted any identical
non-empty aggregate), so the probe was rewritten — see design decisions 8 to 12.
A follow-up review of that fix hardened three further edges: a run lock, the
Linux session-bus branch of the incumbent detector, and reading the identifier
back out of the binary. The current probe was run against the same prototype at
`83ca94a` on 2026-10-21 16:29:15 UTC, with these last edits still in the working
tree, and it printed `PROBE: PASS`:

```text
Binary                     target/native-probe/debug/quotavista-desktop
Application identifier     com.quotavista.prototype.nativeprobe (validation only)
Ordinary identifier unused com.quotavista.prototype
Host platform              darwin (so the channel inspected is a Unix socket,
                           and lsof is the tool that proves a holder)
Validation channel         /tmp/com_quotavista_prototype_nativeprobe_si.sock
Ordinary channel           /tmp/com_quotavista_prototype_si.sock — present and
                           stale at pre-flight, deliberately left in place
Run lock                   /tmp/quotavista-native-probe.lock taken before the
                           first launch, released on cleanup
Cold launch                pid 88938, exec→capabilities 1195 ms, rss 122672 KiB
                           quotavista: capabilities tray=Available notification=Unconfirmed
Step 1b                    ordinary channel stamp before=1792591131 during=1792591131
                           validation channel before=absent during=1792600156
                           (an unchanged stamp supports "no recorded change",
                           not "no activation": connecting to an existing socket
                           need not change its mtime. The substantive guarantee
                           is the disjoint identifier above.)
Second launch              pid 89144, exited status 0 after 119 ms; first survived
                           and logged the SingleInstanceActivation wake
Warm launch                pid 89362, exec→capabilities 441 ms, rss 122256 KiB
No-tray launch             pid 89572, QUOTAVISTA_TRAY=off, 443 ms,
                           quotavista: capabilities tray=Unavailable notification=Unconfirmed
Cold fixture               8|2|2|6|2|2|en/UTC/USD, ids synthetic-kimi-demo
                           synthetic-openrouter-demo, lifecycles `active `
                           facts per scope 3,3 | partitions per scope 1,1
Warm fixture               8|2|2|6|2|2|en/UTC/USD, same ids, lifecycles
                           `needs_credentials ` — all 22 fields (as expected),
                           including every relationship and content check
Termination                SIGTERM, status 143 for cold, warm and no-tray
Cleanup                    default prototype directory untouched; no
                           target/native-probe/debug/quotavista-desktop remains;
                           stale validation socket removed; run lock released
```

The stale ordinary socket, recorded explicitly rather than left implicit: before
launch, `/tmp/com_quotavista_prototype_si.sock` existed with mtime epoch
`1792591131` (2026-10-21 21:58:51 local). `lsof` reported no holder at every
recorded check, so the probe treated it as stale — the shape a socket takes when
its process is terminated by a signal, because the plugin removes it only on
`RunEvent::Exit`. The pathname was deliberately left untouched: cleanup removes
only the validation identity's socket, never the ordinary one, even when that one
is stale. Its recorded mtime was still `1792591131` after the run, and
`/tmp/com_quotavista_prototype_nativeprobe_si.sock` was absent after cleanup —
a statement about the recorded checks, not about ownership during execution.
These are point-in-time observations and do not independently prove absence of
activation. The file was first observed after the reviewer's cross-directory
reproduction; this record does not attribute its origin further than that.

Every capability line and every fixture field agrees with the pre-review run;
what changed is that the run now uses a disjoint single-instance identity and
the fixture is validated field by field — including relationships and content,
not only counts — instead of compared as an aggregate. The
`active ` → `needs_credentials ` lifecycle transition on restart is new
*observable* evidence for the promise `lib.rs` makes about session-only
credentials (design decision 11). Where this subsection and the bullets above
disagree on a pid or a latency, this subsection is the current one.

What macOS does **not** carry: the sixteen interactive observations — window
visibility and the four screens, in-app capability read, notification banners in
both locales, close-to-tray, reopen from the tray, explicit quit, no-tray close
exits, focus after handover, wake merging in automatic mode, refresh-Off wake,
quit cancelling in-flight work, restart as the renderer sees it, real
suspend/resume in both refresh modes, and responsiveness during a slow refresh.
They need a human at the desktop. GUI automation is not a substitute on this
host: AppleScript reports `count windows` → `0` and `window 1` → error `-1719`
for this process, so scripted clicks and keystrokes never reach the app
(the same limitation P1-11 recorded).

Linux actual observations: **none.** Ubuntu cells are `UNVERIFIED`, not
"expected to pass".

CI was explicitly rejected as a substitute, and the workflow file itself says
compilation on CI is not evidence of a native run:

- `workspace-checks` runs on `ubuntu-latest`, a headless hosted runner with no
  GNOME session, no display server, no indicator host, and no notification
  daemon. It also passes `--exclude quotavista-desktop` to clippy and to test
  because the system WebView libraries are absent, so it never even builds the
  desktop application.
- `macos-desktop` runs on `macos-latest`, an unnamed CI image that is not the
  recorded host, and its desktop step is `cargo build` only — a build, which
  this task's PRD forbids treating as desktop usability.

### Criterion 2 — No-tray Linux remains fully usable and exits on window close. Secret-store failures never create plaintext persistence or an implicit session-only downgrade.

**PARTIALLY MET.** Left unchecked in the PRD, because the Linux half was never
observed.

First sentence — *no-tray Linux usable and exits on close*:

- On Linux the no-tray path **is** the default path: `detect_tray` returns
  `Unavailable` for every Linux registration, so `lifecycle::close_decision`
  takes the exit branch rather than hiding the window behind an icon nobody can
  see. That is the conservative direction and it is what this criterion asks
  for.
- The policy is unit-tested (`lifecycle.rs` —
  `closing_the_main_window_exits_when_no_tray_was_detected`) and was observed on
  macOS at the detection level: `QUOTAVISTA_TRAY=off` reported
  `tray=Unavailable`.
- **Not observed anywhere:** that closing the window actually exits the process
  (interactive step 8), and that the four screens remain fully usable with no
  tray on a GNOME desktop. On Linux this is entirely `UNVERIFIED`.

Second sentence — *secret-store failures never create plaintext persistence or
an implicit session-only downgrade*: **MET on macOS, within the scope this
prototype can reach**, on three independent lines of evidence — one static, one
an inspection of the synthetic run, one automated. The scope limit matters: no
native run ever supplied a user credential, because every launch selected the
session-only store and demo seeding writes no secret. What follows therefore
shows that the *storage contract* has no plaintext credential path and that the
*observed synthetic state* holds no secret value. It does not show what happens
to a secret a user actually typed.

- No plaintext persistence, statically: `crates/infrastructure/migrations/0001_persistence.sql`
  is headed "Phase 1 storage boundaries. No credential values or raw provider
  payloads." and declares no credential column in any table. Demo seeding
  (`lib.rs:912 seed_demo_connections`) writes only `connections` and
  `provider_scopes` rows via `create_connection`, which inserts into exactly
  those two tables.
- No plaintext persistence, by inspection of what the run actually produced: the
  read-only store query returns counters and settings only
  (`8|2|2|6|2|2|en/UTC/USD`), the `credential_ref` column is `NULL` on both
  seeded connections, and the state directory holds exactly `journal/`,
  `journal/refresh-counter.v1`, and the three `quotavista.sqlite*` files. The
  inspection covers the SQLite tables the query names and a directory listing; it
  is not a grep of every log and journal body for secret material, and no such
  grep was run.
- No implicit downgrade: `lib.rs:260 build_inner` keeps a credential-store
  failure as `(None, Some(secret_error(&error)))` in `AppState.degraded` and, as
  its comment states, "the session store is never substituted for it".
  `QUOTAVISTA_SECRET_STORE=session-only` is documented at `lib.rs:129-135` as
  "the only way to select the" session-only store, so a failure cannot silently
  become session-only. `lib.rs` prints
  `quotavista: credential store unavailable: …` rather than hiding it, and the
  degraded composition is covered by `a_degraded_composition_reports_the_credential_store_error`
  and `secret_errors_keep_locked_and_unavailable_distinct`, which keep `Locked`
  and `Unavailable` as separate categories.
- Native store actually works on this host: the disposable-service Keychain
  round-trip below.

### Criterion 3 — If a required environment is unavailable, the task and Phase 1 exit remain incomplete with an explicit validation gap; do not claim cross-platform completion.

**MET.** Checked in the PRD.

- `docs/validation/phase-one-exit-assessment.md` opens
  `**Status: INCOMPLETE — Phase 1 does not exit.**`, scores the gate clause by
  clause, and carries a 27-row macOS-versus-Ubuntu matrix in which every Ubuntu
  cell is `UNVERIFIED`.
- `docs/development-plan.md` now records the Phase 1 exit gate as
  **not met** immediately under the gate text, with the reason.
- `docs/phase-one-plan.md`'s status line and exit-review section both record the
  milestone as `INCOMPLETE`, and its task table marks P1-13 "In progress and
  incomplete".
- `docs/traceability.md` gained a `## P1-13 prototype evidence` section, and the
  A10/A11/A21/A23/A25 status cells now name the macOS evidence and the Linux
  gap. **No `Pending` status was converted to `Passed`** — the document's own
  release-evidence rule forbids it, and A11 still reads `Pending; G04`.
- This task's PRD leaves criteria 1 and 2 unchecked and its execution status
  says **incomplete — do not archive**. The Phase 1 parent milestone stays open.
- No cross-platform completion is claimed anywhere in the diff.

### Criterion 4 — Release packaging/signing and the minimum supported macOS matrix remain open in G04 even after prototype checks pass. Required Tauri failures receive documented investigation before a stack change.

**MET.** Checked in the PRD.

- Packaging is not merely undecided, it is demonstrably absent:
  `apps/desktop/src-tauri/tauri.conf.json` sets `"bundle": { "active": false,
  "targets": "all", "icon": [] }`, so there is no `.app`, no `.deb`, no signing
  identity, and no icon. Validation runs `target/debug/quotavista-desktop`
  directly (51.2 MB).
- No minimum macOS version is declared anywhere in the repository: a search for
  `minimumSystemVersion`, `MACOSX_DEPLOYMENT_TARGET`, and
  `LSMinimumSystemVersion` across `apps/`, `crates/`, `docs/`, `.github/`,
  `scripts/`, `README.md`, and `CONTRIBUTING.md` returns no source hit (only
  unrelated build metadata inside `target/`). The recorded host, macOS 27.0.1
  (26A434) on Apple silicon, is an initial validation target, not a declared
  minimum.
- The exit assessment carries a `## G04 remains open` section stating that
  packaging, signing, notarization, installers, and the minimum macOS matrix
  stay open even though prototype checks passed, and that G01, G02, and G04 were
  not closed.
- **No required Tauri failure arose.** Every check below exits 0, and the
  desktop crate builds and runs. The Tauri limitations this task met are
  known, documented in P1-11, and carried as *acceptable degradation* rather
  than as failures: the pinned notification plugin's desktop `permission_state`
  always answers granted without asking the OS (so the adapter reports
  `Unconfirmed` and delivery reports `Queued`, never `Delivered`); Tauri 2.12
  has no desktop suspend/resume event (so `ResumeMonitor` compares monotonic and
  wall clock every 30 s with a two-minute threshold); and a dev build's
  notifications are attributed to the launching terminal rather than to
  QuotaVista, which needs the bundled, signed build that G04 owns. None of these
  prompted a stack change, and none was papered over as a pass.

## Validation commands

Run 2026-10-21 on this branch. Every command below was executed three times:
once during development against the pre-commit base `1761be7`, once as a
confirming battery against the committed revision `1afa2a8`
(`1afa2a88bf18bd6326ae6edc005e2e5477b21dd4`, recorded at the end of this
section), and once more against the rewritten launch probe after the independent
review. All three exit 0 on every command. Where a count differs between runs it
is a volatile number (a pid, a latency, a benchmark percentile) and both values
are shown; the structural counts — 221 workspace tests, 82 frontend tests, 217
catalog keys, 16 pending interactive steps — are identical in all three.

`scripts/test-native-launch-probe.sh` and `scripts/native-launch-lib.sh` exist
only from the review round onward, so they appear in the second and third runs.

```text
cargo fmt --all -- --check                                                    rc=0
cargo clippy --workspace --all-targets --locked -- -D warnings                rc=0
cargo test --locked --workspace                                               rc=0 (221 passed, 0 failed, 1 ignored, 13 suites)
./scripts/check-core-boundaries.sh                                            rc=0 (boundary OK)
./scripts/test-check-core-boundaries.sh                                       rc=0 (5 regression checks)
node ./scripts/check-catalog-parity.mjs                                       rc=0 (217 keys, placeholders matching)
cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check rc=0 (TypeScript contract is current)
./scripts/native-checks.sh --environment                                      rc=0
./scripts/native-checks.sh --automated                                        rc=0 (AUTOMATED: PASS — 21 + 13 + 2 tests)
./scripts/test-native-launch-probe.sh                                         rc=0 (82 regression checks)
./scripts/native-launch-probe.sh                                              rc=0 (PROBE: PASS)
sh -c './scripts/native-checks.sh --interactive < /dev/null'                   rc=0 (16 × OBSERVED: PENDING, 0 passes)
(cd apps/desktop && npm ci)                                                   rc=0
(cd apps/desktop && npm run lint)                                             rc=0
(cd apps/desktop && npm run i18n:check)                                       rc=0 (217 keys)
(cd apps/desktop && npm test)                                                 rc=0 (82 passed, 8 files)
(cd apps/desktop && npm run typecheck)                                        rc=0
(cd apps/desktop && npm run build)                                            rc=0 (dist/assets/index-*.js 287.15 kB)
cargo build --locked -p quotavista-desktop                                    rc=0 (51.2 MB debug binary)
git diff --check                                                              rc=0 (clean)
```

The 1 ignored test is `macos_keychain_synthetic_roundtrip`, which is `#[ignore]`d
because it touches the host keychain.

Manually gated, both exit 0:

```text
QUOTAVISTA_BENCHMARK=1 cargo test --locked -p quotavista-desktop \
  statistics_query_mix_benchmark -- --nocapture                               rc=0
cargo test --locked -p quotavista-infrastructure \
  macos_keychain_synthetic_roundtrip -- --ignored --nocapture                 rc=0
```

Benchmark (A23 local statistics budget), artifact
`apps/desktop/src-tauri/target/p1-12-benchmark.json` (gitignored; its `machine`
and `toolchain` fields are `null`, so the host above is the record):

```text
100,000 facts over 90 settled days (2026-07-01..2026-09-28 UTC), 5 warmups,
30 measured queries through run_request.
overall  p50 847.7 ms   p95 870.6 ms   max 873.2 ms   failures 0
         within_p95_target: true (target 1000 ms)
per mix  A-statistics-90d-day  n=10  p50 851.9  p95 873.2  max 873.2
         B-statistics-90d-week n= 8  p50 783.3  p95 809.9  max 809.9
         C-statistics-30d-day  n= 6  p50 319.3  p95 324.9  max 324.9
         D-overview-90d        n= 6  p50 857.9  p95 870.4  max 870.4
correctness  all four gates asserted (399,570 tokens over 90 days and over
             14 weekly points; 133,170 over 30 days)
cancellation deadline_enforced true at 5000 ms; max abort-to-join 0.74 ms;
             store consistency reproduced after the aborts;
             query_work_cancellation_bound.status = "unverified"
```

P1-12 measured p50 845.7 / p95 866.6 / max 868.6 ms on this host; this run is
p50 847.7 / p95 870.6 / max 873.2 ms. Same machine, same seed, still inside the
1,000 ms target. The development run at `1761be7` measured p50 849.2 / p95
870.6 / max 875.6 ms, so two runs of the same seed differ by a few
milliseconds. These are single-host numbers, not a matrix guarantee.

macOS Keychain smoke check — real host Keychain, disposable service:

```text
service: quotavista-test-p1-05-35988
test secrets::tests::macos_keychain_synthetic_roundtrip ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 66 filtered out
cleanup evidence (printed by the test): qv1:macos-keychain:synthetic-entry-a and
  qv1:macos-keychain:synthetic-entry-b removed from service
  quotavista-test-p1-05-35988; both read back as NeedsCredentials
```

Cleanup verified independently of the test:

```text
security dump-keychain | grep -c 'quotavista-test'                            -> 0
security find-generic-password -s quotavista-test-p1-05-35988 \
  -a 'qv1:macos-keychain:synthetic-entry-a'
  -> security: SecKeychainSearchCopyNext: The specified item could not be found
     in the keychain.
security find-generic-password -s quotavista
  -> security: SecKeychainSearchCopyNext: The specified item could not be found
     in the keychain.
ls "${TMPDIR:-/tmp}/quotavista-prototype"   (before and after the probe)
  -> No such file or directory
```

The last line is the evidence that no real application data directory was
created, read, modified, or deleted at any point.

### Confirming battery at the committed revision

A 25-step script re-ran every command above plus the two manually gated ones at
`1afa2a8`, capturing `git status --porcelain` and `git diff --check` both before
and after so that a check which dirties the tree cannot pass unnoticed. All 25
steps exit 0 (`nonzero steps: 0`); all four git captures are empty.

```text
started   2026-10-21 13:12:50 UTC      finished 2026-10-21 13:13:50 UTC
commit    1afa2a88bf18bd6326ae6edc005e2e5477b21dd4
branch    jondong/p1-13-native-validation
host      Mac18,5  Apple M6  34359738368 bytes
os        macOS 27.0.1 (26A434) arm64
steps     00 git-status-clean      01 git-diff-check        02 fmt
          03 clippy                04 test                  05 core-boundaries
          06 core-boundaries-tests 07 catalog-parity        08 contracts-check
          09 desktop-build         10 native-environment    11 native-automated
          12 native-launch-probe   13 native-interactive    14 npm-ci
          15 npm-lint              16 npm-i18n              17 npm-test
          18 npm-typecheck         19 npm-build             20 benchmark
          21 keychain-smoke        22 keychain-cleanup      23 git-status-after
          24 git-diff-check-after
verdict   every step rc=0; tree clean before and after
```

The probe in step 12 is the run whose observations criterion 1 cites (cold pid
33180, second instance pid 33369, warm pid 33518, no-tray pid 33738), and the
Keychain smoke check in step 21 minted the disposable service
`quotavista-test-p1-05-35988` whose cleanup step 22 verifies. Step 20 rewrote
the benchmark artifact, so the numbers above are this run's, not the
development run's.

This battery re-runs the same checks; it adds no new kind of evidence. It still
contains **zero** interactive observations and **zero** Linux observations, and
step 13 still reports `OBSERVED: PENDING` sixteen times with no passes.

### Confirming battery at `d051409`, and what the first run of it exposed

The review round replaced that runner with an *enforcing* one. Recording each
step's exit status is not a gate: `git status --porcelain` exits 0 on a dirty
tree, the Keychain cleanup queries exit 0 whether or not the entries are gone,
and a script that ends with `cat` returns `cat`'s status. The current runner
asserts the **content** of every required step — 37 assertions over 26 steps —
and exits non-zero if any assertion fails, so `battery rc=0` finally means
something.

```text
started   2026-10-21 16:43:21 UTC      finished 2026-10-21 16:44:24 UTC
commit    d051409ea9f2206d867f0b2eec1f248422c026f0
branch    jondong/p1-13-native-validation
host      Mac18,5  Apple M6  34359738368 bytes
os        macOS 27.0.1 (26A434) arm64
steps     26, including 11b test-native-launch-probe.sh
verdict   BATTERY: PASS — steps with a non-zero exit status: 0,
          failed content assertions: 0, assertions recorded: 37
counts    cargo test 13 suites / 221 passed / 0 failed / 1 ignored (summed over
          the per-suite `test result:` lines, which is the only form cargo
          prints); npm test 8 files / 82 passed; catalog parity 217 keys;
          test-native-launch-probe.sh 82 ok; native-launch-probe.sh
          PROBE: PASS with 9 RESULT: PASS; native-checks.sh --interactive
          16 OBSERVED: PENDING and 0 passes; benchmark p95 869.7 ms against the
          1,000 ms target
keychain  service quotavista-test-p1-05-63429; cleanup verified by
          `security dump-keychain | grep -c quotavista-test` = 0, three
          `could not be found in the keychain` results (entry-a, entry-b, and
          the production `quotavista` service), and the prototype's default
          state directory absent
git       status and `git diff --check` empty before and after, so no check
          dirtied the tree
```

The first run of this battery at the same commit reported
`BATTERY: FAIL (7 assertion(s), 0 step(s))`. **All seven failures were defects in
the runner, not in the repository**, and they are recorded here because a gate
that fails for the wrong reason is a gate nobody trusts. `a_count` and `a_none`
captured their match count as `$(grep -Ec … || echo 0)`; `grep -c` prints `0`
*and* exits 1 when nothing matches, so the `||` appended a second line and the
variable held the two-line string `0\n0`, which breaks `[ "$got" -eq 0 ]`. Every
one of the seven was an assertion that had correctly found zero matches —
clippy errors, `test result: FAILED`, `panicked`, self-test `FAIL` lines,
`PROBE: FAIL|REFUSED`, interactive `OBSERVED: PASS`, and benchmark failures.

Rather than assume that, each pattern was re-run by hand against the same logs
(all returned `count=0` with `grep_rc=1`), the helpers were changed to
`) || true` plus `got=${got:-0}`, and all seven were replayed against the
*existing* logs: every one passed, while the exact-count assertions that must
not weaken still held (16 `OBSERVED: PENDING`, 9 probe `RESULT: PASS`, 3
automated `RESULT: PASS`). The battery was then re-run end to end, which is the
`BATTERY: PASS` recorded above.

An `a_cargo_totals` helper was added for the same reason: `cargo test` prints one
`test result:` line per suite and never a single total, so an assertion written
against `test result: ok\. 221 passed` can never match. The helper sums the
per-suite lines and asserts the sum, which is how the `13 suites / 221 passed /
0 failed / 1 ignored` figure above is derived rather than eyeballed.

This is still the same evidence, re-run: **zero** interactive observations and
**zero** Linux observations. Step 13 reports `OBSERVED: PENDING` sixteen times
with no passes, and no step in the battery can change that.

## Pending and not claimed

- **Ubuntu 26.04 GNOME x86_64 native run — blocker, no owner yet.** Nothing was
  built, run, or observed on Linux. `scripts/native-checks.sh --environment` is
  what records the distribution, `XDG_SESSION_TYPE` (wayland or x11),
  `XDG_CURRENT_DESKTOP`, the GNOME Shell version, and the `gdbus`
  StatusNotifierWatcher and `org.freedesktop.Notifications` probes. The full
  operator procedure, including provisioning and synthetic setup/reset, is
  `docs/validation/native-target-procedure.md`. Until it runs, gate clauses 2 and
  4 are unverified, A11 cannot move, and Phase 1 does not exit.
- **Sixteen interactive macOS observations — blocker, needs a human at the
  desktop.** Run `./scripts/native-checks.sh --interactive` from a real terminal
  and type the actual observation for each step. Recorded as `PENDING`, never as
  passed. GUI automation on this host cannot substitute (AppleScript sees zero
  windows for the process).
- **Linux tray availability — undecided.** `detect_tray` reports `Unavailable` on
  Linux even after successful registration. Promoting it needs a
  StatusNotifierWatcher host probe validated by a native run. The conservative
  answer stands and is the safe direction.
- **Linux Secret Service — never executed.**
  `linux_secret_service_synthetic_roundtrip` is the twin of the macOS smoke
  check; on a host with no session bus it prints a `pending:` line and returns
  rather than failing. It has never run anywhere, so the Linux half of the
  native-store row is unverified.
- **Locked-store and unavailable-store native observation — deliberately not
  attempted.** Producing it would mean locking the operator's login keychain.
  Automated coverage is cited instead and the row is marked `PARTIAL`.
- **Query-work cancellation bound — owned follow-up, carried from P1-12.** The
  statistics path has no cooperative cancellation token, so `JoinHandle::abort`
  stops work only at the next await point and a query already inside the
  synchronous aggregation runs to completion. The ≤0.74 ms abort-to-join figure
  measures consumer-facing responsiveness, not stopped work. Owner: Phase 2
  bounded-work hardening (a cooperative token through `query_statistics` into
  `statistics::aggregate`). Not a phase-exit blocker: the 5,000 ms deadline is
  enforced and committed data survives the aborts.
- **zh-CN failure-scenario matrix — still deferred.** P1-12 deferred it to these
  native/runtime checks; the interactive steps were not run, so it remains
  deferred. Catalog parity (217 keys) and the jsdom locale suites pass, but no
  native locale switch or zh-CN notification banner was observed.
- **A01–A04, G01, G02 — unverified by design.** No real provider was contacted.
  G04 remains open. This task closes no release acceptance criterion.
- **CI was not run.** The work is local commits on this branch; no push and no
  pull request were made, per instruction. CI would not have produced native
  evidence in any case.
- **Single-host benchmark.** All timings come from one Apple M6 machine and are
  not a performance guarantee across a support matrix.
- **No packaging, signing, notarization, or installer evidence exists.**
  Bundling is disabled in the prototype configuration; that work belongs to G04.
- **What the review round changed, and what it did not.** An independent review
  of `10dc053` returned CHANGES_REQUESTED with two blocking findings: the probe's
  fresh `TMPDIR` did not isolate the single-instance channel (B1), and its store
  inspection accepted any identical non-empty aggregate (B2) — the reviewer
  demonstrated `PROBE: PASS` against a disposable double that read back
  `0|0|0|0|0|0|zh-CN/UTC/USD`. Both are fixed, with an 82-check regression suite
  (`scripts/test-native-launch-probe.sh`) that reproduces each failure shape. A
  follow-up review of that fix hardened three further edges: a run lock, so two
  overlapping probes cannot share the fixed validation identifier; the Linux
  session-bus branch of the incumbent detector, because a socket-only detector
  reports "clear" on Linux no matter what is running; and reading the identifier
  back out of the binary, so `--skip-build` cannot reuse an artifact built for the
  ordinary identity. It also corrected an overstatement in this record — an
  unchanged socket stamp supports "no recorded change", not "no activation" — and
  narrowed the claims the exit assessment made about the PASS rows. Two
  further findings were documentation scope (S1: the interactive checklist printed
  bare `cargo run` commands that would select the native store; S2: "no plaintext
  credential persistence" claimed more than the inspection showed) plus a nit
  about the Keychain prefix rule and the read-only cleanup queries. None of this
  adds a single interactive or Linux observation: the sixteen macOS steps are
  still `PENDING`, every Linux row is still `UNVERIFIED`, and Phase 1 still does
  not exit.
