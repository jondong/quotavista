# P1-13: Validate native macOS and Ubuntu prototype behavior

## Goal

Decide whether the chosen stack is viable on the actual target desktops.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-12](../archive/2026-10/10-18-p1-12-integration-validation/prd.md)

Acceptance mapping: A10, A11, A21, A23, A25; G04 prototype evidence. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Create an evidence matrix for the available macOS host and Ubuntu 26.04/GNOME/x86_64; record exact OS, architecture, Linux session type, toolchains, and procedure.
- Check cold/warm launch, reopen/single instance, tray present/absent, notifications present/absent, close/quit, wake, secret store locked/unavailable/session-only, persistence/restart, and both UI locales.
- Write a Phase 1 exit assessment with passed/failed/unverified rows, blockers, acceptable degradation, and the next Phase 2 entry conditions.

## Acceptance criteria

- [ ] Native evidence includes actual observations for each target; Linux is not marked passed from CI or macOS behavior.
- [ ] No-tray Linux remains fully usable and exits on window close. Secret-store failures never create plaintext persistence or an implicit session-only downgrade.
- [x] If a required environment is unavailable, the task and Phase 1 exit remain incomplete with an explicit validation gap; do not claim cross-platform completion.
- [x] Release packaging/signing and the minimum supported macOS matrix remain open in G04 even after prototype checks pass. Required Tauri failures receive documented investigation before a stack change.

## Out of scope

No Windows support, live-provider access, installers/notarization release acceptance, or claim that build success proves desktop usability.

## Execution status

Started 2026-10-21 and **incomplete — do not archive**. Criteria 3 and 4 are met: the unavailable Ubuntu 26.04 GNOME x86_64 environment is recorded as an explicit validation gap in [the Phase 1 exit assessment](../../../docs/validation/phase-one-exit-assessment.md) instead of being covered by a cross-platform claim, and G04 stays open — `tauri.conf.json` sets `bundle.active: false` with an empty icon list, and no minimum macOS version is declared anywhere in the repository. No required Tauri failure arose, so no stack change was considered. Criteria 1 and 2 stay unchecked on purpose: only the macOS target carries actual observations, and the sixteen interactive desktop steps in `scripts/native-checks.sh --interactive` were not performed by a human on any target. Evidence, commands, and the remaining gaps are in [validation.md](validation.md); the operator procedure is [native-target-procedure.md](../../../docs/validation/native-target-procedure.md).
