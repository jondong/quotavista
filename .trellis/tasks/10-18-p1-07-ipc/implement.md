# P1-07 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-05](../10-18-p1-05-secret-stores/prd.md), [P1-06](../10-18-p1-06-simulation/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Compose the application services and map reviewed commands to ports.
- [ ] Add explicit capabilities/permissions and typed frontend client/subscriptions.
- [ ] Exercise boundary validation, revision conflicts, safe errors, and secret exclusion end to end.

## Validation and review

- [ ] Command-boundary tests and contract-drift checks.
- [ ] Inspect Tauri capability configuration and run local IPC smoke checks; frontend type check passes.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
