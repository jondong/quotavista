# P1-13: Validate native macOS and Ubuntu prototype behavior

## Goal

Decide whether the chosen stack is viable on the actual target desktops.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-12](../10-18-p1-12-integration-validation/prd.md)

Acceptance mapping: A10, A11, A21, A23, A25; G04 prototype evidence. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Create an evidence matrix for the available macOS host and Ubuntu 26.04/GNOME/x86_64; record exact OS, architecture, Linux session type, toolchains, and procedure.
- Check cold/warm launch, reopen/single instance, tray present/absent, notifications present/absent, close/quit, wake, secret store locked/unavailable/session-only, persistence/restart, and both UI locales.
- Write a Phase 1 exit assessment with passed/failed/unverified rows, blockers, acceptable degradation, and the next Phase 2 entry conditions.

## Acceptance criteria

- [ ] Native evidence includes actual observations for each target; Linux is not marked passed from CI or macOS behavior.
- [ ] No-tray Linux remains fully usable and exits on window close. Secret-store failures never create plaintext persistence or an implicit session-only downgrade.
- [ ] If a required environment is unavailable, the task and Phase 1 exit remain incomplete with an explicit validation gap; do not claim cross-platform completion.
- [ ] Release packaging/signing and the minimum supported macOS matrix remain open in G04 even after prototype checks pass. Required Tauri failures receive documented investigation before a stack change.

## Out of scope

No Windows support, live-provider access, installers/notarization release acceptance, or claim that build success proves desktop usability.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
