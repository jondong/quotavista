# P1-05 execution plan

## Entry checks

- [x] Read the PRD, design, relevant specs, and reviewed baseline.
- [x] Confirm predecessor tasks are completed, including archived records: [P1-04](../archive/2026-10/10-18-p1-04-recovery-fences/prd.md)
- [x] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [x] Check current official crate APIs and pin compatible native-store versions.
- [x] Implement adapters and journal-backed lifecycle integration; add session-only mode.
- [x] Exercise fake-store failures and available macOS native checks with disposable synthetic secrets; remove test entries.

## Validation and review

- [x] Fake-store crash/rotation/locked/unavailable tests and session-only restart checks.
- [x] Native smoke checks use disposable synthetic values only; record host and cleanup evidence.
- [x] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [x] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
