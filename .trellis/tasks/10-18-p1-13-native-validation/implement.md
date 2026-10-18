# P1-13 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-12](../10-18-p1-12-integration-validation/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Prepare a reproducible native checklist and synthetic setup/reset instructions.
- [ ] Run available target checks and collect sanitized evidence; keep unavailable rows explicitly unverified.
- [ ] Assess Phase 1 exit criteria and update development-plan/traceability status without closing later gates.

## Validation and review

- [ ] Native checklist results tied to commit, host, session type, and exact steps.
- [ ] Phase 1 exit review linking evidence and unresolved blockers.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
