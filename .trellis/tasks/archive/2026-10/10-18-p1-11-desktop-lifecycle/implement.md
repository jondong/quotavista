# P1-11 execution plan

## Entry checks

- [x] Read the PRD, design, relevant specs, and reviewed baseline.
- [x] Confirm predecessor tasks are completed, including archived records: [P1-07](../archive/2026-10/10-18-p1-07-ipc/prd.md), [P1-08](../archive/2026-10/10-18-p1-08-bilingual-shell/prd.md)
- [x] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [x] Implement lifecycle ports and explicit close/reopen/quit policy.
- [x] Add tray and synthetic notification capability adapters with visible fallback.
- [x] Run available native smoke checks and prepare reproducible Linux checks for P1-13.

## Validation and review

- [x] Coordinator/lifecycle tests for shutdown, wake, and Off mode.
- [x] Native window/tray/notification checks and a no-tray scenario; record observed results only.
- [x] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [x] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
