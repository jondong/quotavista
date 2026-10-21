# P1-12 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-09](../archive/2026-10/10-18-p1-09-overview-statistics/prd.md), [P1-10](../archive/2026-10/10-18-p1-10-connections-settings/prd.md), [P1-11](../archive/2026-10/10-18-p1-11-desktop-lifecycle/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Run the full documented check suite and integrated bilingual failure scenarios.
- [ ] Measure local query workload and cancellation; investigate failures or file explicit blockers.
- [ ] Review the diff and update traceability, specs, contributor commands, and prototype limitations.

## Validation and review

- [ ] All commands established in P1-01 plus catalog and contract drift checks.
- [ ] Recorded integration scenario results and benchmark report with raw or reproducible measurements.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
