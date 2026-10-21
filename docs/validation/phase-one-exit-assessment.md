# Phase 1 exit assessment

**Status: INCOMPLETE — Phase 1 does not exit.**

Assessed 2026-10-21 (UTC) by Trellis task P1-13 on branch
`jondong/p1-13-native-validation`. Prototype revision under test: `1761be7`
(the merged P1-12 head), re-confirmed by a full 25-step battery at this task's own commit `1afa2a8`. P1-13 changes no application source file — it adds
`scripts/native-launch-probe.sh`, this assessment, and
[native-target-procedure.md](native-target-procedure.md) — so every observation
below describes the same prototype that P1-12 validated.

The exit gate in [development-plan.md](../development-plan.md) reads:

> The prototype runs on target macOS and Linux environments, credentials are not
> persisted in plaintext, and Linux remains usable without a tray. If Linux is
> unavailable for validation, identify that gap instead of claiming
> cross-platform completion.

One of the two required native environments was unavailable, and the desktop
observations that need a human at the machine were not made on the environment
that was available. This document identifies that gap, as the gate instructs,
instead of claiming cross-platform completion.

## Gate clauses

| # | Clause | Result | Why |
| --- | --- | --- | --- |
| 1 | The prototype runs on the target macOS environment | **Partial** | Non-interactive native launches were observed on the recorded host (start-up, capability detection, single instance, restart, no-tray mode). The sixteen interactive desktop observations were not made: they need a human at the desktop, and GUI automation of this app's windows is not available on this host. |
| 2 | The prototype runs on the target Linux environment | **Unverified** | No Ubuntu 26.04 GNOME x86_64 machine was available. Nothing was run, built, or observed on Linux. CI's `ubuntu-latest` job is not a substitute: it is a headless hosted runner with no GNOME session and it excludes the desktop crate entirely. |
| 3 | Credentials are not persisted in plaintext | **Partial on macOS** | The storage contract is static and unit-tested, and the inspected artifacts of the synthetic session-only runs contain no credential value. That is narrower than the clause asks: no native run ever supplied a user credential, so nothing here shows what happens to a secret a user did enter. Unverified on Linux, where the Secret Service adapter has never run. |
| 4 | Linux remains usable without a tray | **Unverified** | The no-tray close policy is unit-tested and was observed on macOS through `QUOTAVISTA_TRAY=off`, and `detect_tray` reports `Unavailable` on Linux by design, so the no-tray path *is* the Linux path. That it is actually usable on a GNOME desktop has not been observed. |
| 5 | Identify the gap instead of claiming cross-platform completion | **Done** | This document, plus the `PENDING`/`UNVERIFIED` rows below and the blockers section. |

## Evidence matrix

Column meanings: **PASS** = actually observed on that target; **PARTIAL** =
observed only at a level below the user-visible behaviour the criterion asks
about; **PENDING** = the check exists and can be run, but nobody ran it on that
target; **UNVERIFIED** = no evidence of any kind from that target.

`--automated` and the workspace/npm suites are host-independent source checks.
They ran on macOS only. A green run on macOS says nothing about Linux, so every
Linux cell below is `UNVERIFIED` rather than "expected to pass".

| Check | macOS 27.0.1 (26A434), arm64 | Ubuntu 26.04, GNOME, x86_64 |
| --- | --- | --- |
| Cold launch: binary starts, composition initialises | **PASS** (process/startup probe, not a rendered window) — 1,174 ms from `exec` to `quotavista: capabilities tray=Available notification=Unconfirmed` | **UNVERIFIED** |
| Cold launch: window opens, four screens reachable | **PENDING** — interactive step 1 | **UNVERIFIED** |
| Warm launch on the same state directory | **PASS** — 454 ms, no reseed | **UNVERIFIED** |
| Local recovery runs before scheduling | **PARTIAL** — automated (`a_corrupt_lifecycle_journal_blocks_the_composition_before_scheduling`); no native corrupt-journal launch was performed | **UNVERIFIED** |
| Single instance: second launch hands over | **PASS** (process-level) — the second process (pid 33369) exited 0 after 132 ms, the first process was still running afterwards and had logged `quotavista: wake SingleInstanceActivation mode Off permitted false merged 0 not-due 0`. This says nothing about whether a window appeared | **UNVERIFIED** |
| Single instance: first window shown and focused | **PENDING** — interactive step 9 | **UNVERIFIED** |
| Tray present and visible | **PARTIAL** — detection reports `tray=Available`; visibility, menu, and reopen are interactive steps 5–7 | **UNVERIFIED** — the adapter reports `Unavailable` on Linux by design, so there is no tray to observe until a validated host probe exists |
| Tray absent (`QUOTAVISTA_TRAY=off`) | **PASS** (forced branch) — with the one-way override set, detection reports `tray=Unavailable`. This exercises the override's forced-unavailable path; it is not the discovery of a genuinely missing tray host, which on Linux is what `detect_tray` reports by design | **UNVERIFIED** — this is the Linux default path |
| Closing the window with no tray exits the process | **PENDING** — interactive step 8; policy unit-tested by `closing_the_main_window_exits_when_no_tray_was_detected` | **UNVERIFIED** |
| Notification availability | **PARTIAL** — detection reports `notification=Unconfirmed` because the pinned plugin cannot ask the operating system; banners are interactive steps 3–4 | **UNVERIFIED** |
| Explicit quit from the tray | **PENDING** — interactive step 7. The probe's `SIGTERM` exit (status 143) is *not* the tray Quit path and is not evidence for it | **UNVERIFIED** |
| Quit cancels in-flight work | **PENDING** — interactive step 12 | **UNVERIFIED** |
| Wake merges refresh work (automatic mode) | **PENDING** — interactive step 10 | **UNVERIFIED** |
| Wake starts nothing when refresh is Off | Split in two. Automated lifecycle-policy coverage: **PASS** (`refresh_off_never_initiates_an_implicit_query_on_wake`). Native Off-mode wake: **PENDING** — interactive step 11. What was observed natively is Off gating on a single-instance handover (`mode Off permitted false merged 0 not-due 0`), which is not a deliberate Off-mode wake and not a sleep cycle | **UNVERIFIED** |
| Real suspend/resume, automatic refresh on | **PENDING** — interactive step 14 (must exceed the two-minute `RESUME_THRESHOLD`) | **UNVERIFIED** |
| Real suspend/resume, refresh Off | **PENDING** — interactive step 15 | **UNVERIFIED** |
| Window stays responsive during a slow refresh | **PENDING** — interactive step 16 | **UNVERIFIED** |
| Secret store: explicit session-only selection | **PASS** (explicit selection only) — every scripted launch set `QUOTAVISTA_SECRET_STORE=session-only` and the run stayed in memory. This does **not** validate automatic behaviour after a native store failure: no fallback is attempted, and the composition keeps the store error in `AppState.degraded` rather than substituting a session store | **UNVERIFIED** |
| Secret store: native backend round-trip | **PASS** — `macos_keychain_synthetic_roundtrip` against the real Keychain with disposable service `quotavista-test-p1-05-35988`; cleanup independently verified (`security dump-keychain \| grep -c 'quotavista-test'` → 0) | **UNVERIFIED** — `linux_secret_service_synthetic_roundtrip` has never run |
| Secret store: locked and unavailable stay distinct | **PARTIAL** — automated only (`secret_errors_keep_locked_and_unavailable_distinct`, `a_degraded_composition_reports_the_credential_store_error`). Native observation was deliberately not attempted: it would mean locking the operator's own login keychain | **UNVERIFIED** |
| No plaintext credential persistence | **PARTIAL** — the storage *contract* is static and unit-tested: `0001_persistence.sql` declares no credential column ("No credential values or raw provider payloads"), and demo seeding writes only `connections` and `provider_scopes`. What the native runs actually inspected was narrower: the read-back store row `8\|2\|2\|6\|2\|2\|en/UTC/USD`, zero non-null `credential_ref`, and the journal directory listing. No native run ever supplied a user credential (the store was session-only and seeding writes no secret), so no observation here shows what happens to a secret a user did enter, and no log or journal body was grepped for secret material | **UNVERIFIED** |
| Persistence: committed data survives a restart | **PARTIAL** — the store row is byte-identical across the cold and warm launches (same `data_revision` 8, same settings); the renderer's own view of it is interactive step 13 | **UNVERIFIED** |
| Both UI locales at runtime | **PARTIAL** — en/zh-CN catalog parity (217 keys, placeholders matching) and the jsdom locale suites pass, and the seeded settings row reads `en`. Native language switching and the zh-CN notification banner are interactive steps 3–4 | **UNVERIFIED** |
| Local statistics query budget (one A23 sub-clause) | **PASS** for this sub-clause only, not for A23 as a whole — 100,000 synthetic facts, 5 warmups, 30 measured queries on this host; overall p50 847.7 ms, **p95 870.6 ms** against the 1,000 ms target, max 873.2 ms, 0 failures, all four correctness gates asserted. A23's other clauses (navigation during slow work, disk-full, interrupted migration) are separate rows | **UNVERIFIED** |
| Query-work cancellation bound | **UNVERIFIED** — no cooperative cancellation token exists inside aggregation; consumer-facing abort resolved in ≤0.74 ms under the enforced 5,000 ms deadline, which measures responsiveness, not stopped work | **UNVERIFIED** |
| Workspace and frontend suites | **PASS** — `cargo fmt`/`clippy -D warnings`/`test`, both boundary scripts, catalog parity, contract drift, `npm ci`/`lint`/`i18n:check`/`test`/`typecheck`/`build`, `git diff --check`, all exit 0 | **UNVERIFIED** — never run on Linux; CI's Linux job excludes the desktop crate because the system WebView libraries are absent |
| Desktop debug build | **PASS** — `cargo build --locked -p quotavista-desktop`, 51.2 MB binary | **UNVERIFIED** |
| Single-instance isolation from an ordinary running app | **PASS** — the probe builds and runs a validation-only identifier (`com.quotavista.prototype.nativeprobe`) whose channel `/tmp/com_quotavista_prototype_nativeprobe_si.sock` is disjoint from `/tmp/com_quotavista_prototype_si.sock`, so the two cannot address each other by construction. Step 1b additionally records that the ordinary socket's stamp was unchanged across the run; that is a point-in-time observation supporting "no recorded change", not proof of "no activation", because connecting to an existing socket need not change its mtime. A fail-closed gate refuses to launch over any incumbent or over a channel state it cannot interpret, an atomic `mkdir` lock stops two overlapping runs sharing the fixed validation identifier, and the identifier is read back out of the binary so `--skip-build` cannot reuse an ordinary-identity artifact. The regression suite covers the detection logic including a live socket, a stale socket, an uninspectable one, a held validation socket that cleanup must not unlink, the three `ps` argv spellings, and the lock's live-owner, dead-owner and foreign-release cases | **UNVERIFIED** — the Linux channel is the session D-Bus name `<identifier>.SingleInstance`, which the same identifier mechanism isolates, and the detector queries it with `org.freedesktop.DBus.NameHasOwner` rather than looking for a socket; that branch is covered by the regression suite under a shadowed platform, but no Linux run has exercised it on a real session bus |
| Seeded fixture actually initialised (not just non-empty) | **PASS** — the probe validates revision, connection identities, lifecycles, scope identities and key/type, orphan scopes, credential references, fact/partition/sync counts and settings per launch, plus relationships and content: facts joined to a partition of a different scope, published partitions pointing at another partition's run, facts and partitions per scope, fact kinds, and that each scope's amounts are distinct, numeric, positive and contiguous. The regression suite includes six count-preserving corruptions that keep the aggregate at `8\|2\|2\|6\|2\|2\|en/UTC/USD` and are still rejected, alongside schema-only, partial-seed, wrong-identity and warm-mismatch stores | **UNVERIFIED** |

Full command output, host capture, and per-criterion detail are in the
[P1-13 validation record](../../.trellis/tasks/10-18-p1-13-native-validation/validation.md).

## Blockers

1. **No Ubuntu 26.04 GNOME x86_64 host (blocks the exit).** Gate clause 2 and
   clause 4 cannot be assessed, A11 cannot move, and the Linux Secret Service
   adapter has never executed anywhere. Nothing about Linux may be inferred from
   the macOS run or from CI. Resolution: run
   [native-target-procedure.md](native-target-procedure.md) on a real Ubuntu
   26.04 GNOME x86_64 desktop and record the session type.
2. **Sixteen interactive desktop observations were not made on macOS (blocks the
   exit).** Window visibility, tray menu behaviour, notification banners in both
   locales, close/reopen/quit, wake merging, real suspend/resume, and window
   responsiveness all require a human at the desktop. GUI automation is not a
   workaround on this host: AppleScript sees zero windows for the process
   (`count windows` → `0`, `window 1` → error `-1719`), so scripted clicks and
   keystrokes never reach the app. Resolution: an operator runs
   `./scripts/native-checks.sh --interactive` from a real terminal and types the
   actual observation for each step.
3. **Linux tray availability is undecided (blocks A11's tray row).**
   `detect_tray` reports `Unavailable` on Linux because libappindicator
   registration succeeds with no indicator host, so registration is not proof of
   a visible icon. Promoting Linux to `Available` needs a host probe validated by
   a native run — exactly the run that blocker 1 prevents. Until then the
   conservative answer stands, and it is the safe direction: closing the window
   exits instead of hiding behind an icon nobody can see.
4. **Query-work cancellation bound (carried from P1-12, owned follow-up).** The
   benchmark artifact records `query_work_cancellation_bound.status =
   "unverified"`. Not a phase-exit blocker — the 5,000 ms deadline is enforced and
   committed data survives aborts — but it is an open A23/architecture bounded-work
   item owned by Phase 2 bounded-work hardening (a cooperative token through
   `query_statistics` into `statistics::aggregate`).

## Acceptable degradation

Recorded as deliberate, safe, and honest for a prototype. None of these is a
defect to fix before Phase 2 entry, but none may be described as a capability the
prototype does not have.

- **Linux tray reported unavailable even where an indicator host exists.**
  Conservative; keeps the no-tray close policy, which is the behaviour the gate
  actually requires on Linux.
- **Notifications reported `Unconfirmed`, delivery reported `Queued`.** The
  pinned plugin's desktop `permission_state` always answers granted without
  asking the operating system, and `show` discards the OS response. Claiming
  `Available`/`Delivered` would be a promise nobody keeps. The UI says so in both
  locales, and a queued verdict next to a visible banner is the expected result.
- **Tray labels stay English in both locales.** The tray registers once at
  start-up while locale switching lives in the renderer; localised labels would
  need settings-driven re-registration. Notification text *is* localised because
  it is rendered per request.
- **macOS dev-build notifications are attributed to the launching terminal, not
  to QuotaVista.** Attributed notifications need a bundled, signed build — G04
  release work, explicitly out of Phase 1 scope.
- **No installer, no bundling, no signing.** `tauri.conf.json` sets
  `bundle.active: false` with an empty icon list, and no minimum macOS version is
  declared anywhere in the repository (no `minimumSystemVersion`, no
  `MACOSX_DEPLOYMENT_TARGET`, no `LSMinimumSystemVersion`). Validation therefore
  runs the debug binary directly. That is acceptable for a prototype and is
  exactly the work G04 retains.
- **Benchmark numbers come from one machine** (Apple M6, 32 GB). They are a
  single-host measurement, not a performance guarantee across the support matrix.

## G04 remains open

Release packaging, signing, notarization, installers, and the minimum supported
macOS matrix stay open in G04 even though prototype checks passed. The recorded
host is macOS 27.0.1 (26A434) on Apple silicon — an initial validation target,
not a declared minimum. No requirement in this assessment closes G04, G01, or
G02, and no `Pending` status in [traceability.md](../traceability.md) was
converted to `Passed`.

## Phase 2 entry conditions

Phase 2 (initial provider validation) may start on the parts that do not depend
on native desktop evidence — provider authorization research, capability
reports, sanitized fixtures — because it needs no Ubuntu machine and no
interactive desktop session. The following must be recorded before the **Phase 1
milestone** is closed and before any cross-platform claim is made:

1. An Ubuntu 26.04 LTS, GNOME, x86_64 desktop session runs the full procedure in
   [native-target-procedure.md](native-target-procedure.md) at a named commit:
   `--environment` (including `XDG_SESSION_TYPE` = wayland or x11,
   `XDG_CURRENT_DESKTOP`, GNOME Shell version, and both `gdbus` probes),
   `--automated`, `native-launch-probe.sh`, and all sixteen `--interactive` steps
   with typed observations.
2. `linux_secret_service_synthetic_roundtrip` runs on that host with its cleanup
   independently verified, closing the Linux half of the native-store row.
3. A decision on Linux tray availability: either keep `Unavailable` with the
   reason recorded, or implement a StatusNotifierWatcher host probe and validate
   it on that native run. The recorded `gdbus` probe output is the evidence
   either way.
4. A macOS operator completes the same sixteen interactive steps at the same
   commit, so both targets carry actual observations rather than one target
   carrying the other's inference.
5. The A23 cancellation bound is either accepted as a documented Phase 2
   limitation with its owner named, or the cooperative token lands.
6. This assessment is updated with the new rows and re-decided. The Phase 1
   parent task stays open until then, and P1-13 stays incomplete.

## Environment availability at assessment time

Recorded so a later reader knows what could and could not have been used.
Verified 2026-10-21 from official sources.

**Ubuntu 26.04 LTS ("Resolute Raccoon") is released and downloadable.** Canonical
announced it on 2026-04-23; standard support runs to April 2031 (ten years with
Ubuntu Pro ESM). Supported architectures include amd64 (x86_64) and arm64. The
desktop download page currently offers **Ubuntu 26.04.1 LTS** as the latest LTS
point release, shipping GNOME 50 and Linux kernel 7.0, with a minimum of a 2 GHz
dual-core CPU, 6 GB RAM, and 25 GB disk. An operator provisioning this target
should record the exact `VERSION_ID` from `/etc/os-release` — 26.04 and 26.04.1
are both "Ubuntu 26.04" for the purposes of the requirement, and the point
release must not be silently substituted into the record. (The release schedule
page lists the 26.04.1 point release under week 46, 27 August 2026, while the
April announcement email said 4 August; the discrepancy is noted rather than
resolved, because neither date affects this assessment.)

**The macOS host is current.** `sw_vers` reports macOS 27.0.1, build 26A434,
which Apple published on 2026-09-28 as the macOS Golden Gate 27.0.1 security
update; macOS 27.0 shipped 2026-09-14. macOS 27.2 was still in release-candidate
form on 2026-10-19 (build 26B118) and had not shipped, so 27.0.1 is the newest
public release and the host is not behind. macOS Golden Gate 27 supports Apple
silicon Macs from 2020 onward (MacBook Neo 2026, MacBook Air/Pro with Apple
silicon 2020+, iMac with Apple silicon 2021+, Mac mini with Apple silicon 2020+,
Mac Studio 2022+, Mac Pro with Apple silicon 2023). This host is `Mac18,5` with
an Apple M6 and 32 GB, inside that set. **None of this establishes a minimum
supported macOS version for QuotaVista** — that decision belongs to G04.

**No Linux host existed in this session.** No VM, container, or remote machine
was provisioned. A container would not have satisfied the requirement anyway:
the target is a GNOME desktop session with a window server, an indicator host,
and a notification daemon, and a headless build is explicitly insufficient.

### Sources

- Canonical, "Canonical releases Ubuntu 26.04 LTS Resolute Raccoon", 2026-04-23.
- ubuntu-announce, "Ubuntu 26.04 ('Resolute Raccoon') LTS released", 2026-04-23.
- Ubuntu release notes and release schedule for 26.04 (support lifespan,
  architectures, point-release timing).
- ubuntu.com/download/desktop (26.04.1 LTS, GNOME 50, kernel 7.0, minimum
  requirements) and ubuntu.com/about/release-cycle.
- Apple Support, "Apple security releases" and "Find out which macOS your Mac is
  using" (27.0.1 / 26A434 dated 2026-09-28; supported hardware list).
- macosdb.com macOS releases table and 9to5Mac, 2026-10-19 (macOS 27.2 RC 1 build
  26B118 not yet shipped).
- Host commands: `sw_vers`, `uname -srm`, `sysctl -n hw.model
  machdep.cpu.brand_string hw.memsize`.
