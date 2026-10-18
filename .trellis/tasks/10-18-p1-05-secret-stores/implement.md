# P1-05 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-04](../10-18-p1-04-recovery-fences/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Check current official crate APIs and pin compatible native-store versions.
- [ ] Implement adapters and journal-backed lifecycle integration; add session-only mode.
- [ ] Exercise fake-store failures and available macOS native checks with disposable synthetic secrets; remove test entries.

## Validation and review

- [ ] Fake-store crash/rotation/locked/unavailable tests and session-only restart checks.
- [ ] Native smoke checks use disposable synthetic values only; record host and cleanup evidence.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
