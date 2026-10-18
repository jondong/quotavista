# P1-03 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-02](../10-18-p1-02-contracts/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Create migrations and repository ports/adapters with exact numeric encodings.
- [ ] Implement settings/current-state persistence and candidate/publication operations.
- [ ] Fault-test rollback, reopen, duplicate/corrected records, and partial replacement; integrate checks.

## Validation and review

- [ ] Temporary-database integration tests for rollback, restart, publication, removed facts, and pool settings.
- [ ] Workspace tests and SQLx migration validation using the reproducible setup.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
