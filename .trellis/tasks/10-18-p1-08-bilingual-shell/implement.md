# P1-08 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-02](../10-18-p1-02-contracts/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Implement navigation and accessible view foundations.
- [ ] Implement locale selection/catalogs/formatters and parity command; integrate the CI check.
- [ ] Verify both languages and state-preserving switching in the renderer; record source conventions.

## Validation and review

- [ ] Frontend type/lint/build and catalog-parity checks.
- [ ] Interaction checks for locale selection, live switching, focus, resizing, and accessible labels.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
