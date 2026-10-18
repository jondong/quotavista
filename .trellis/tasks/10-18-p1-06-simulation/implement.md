# P1-06 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-04](../10-18-p1-04-recovery-fences/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Add fixture scenarios and a network-disabled simulated adapter.
- [ ] Implement a bounded coordinator through core ports and repository operations.
- [ ] Run deterministic scheduling, freshness, cooldown, cancellation, and partial-history tests.

## Validation and review

- [ ] Fake-clock tests for manual-only mode, coalescing, concurrency, wake behavior, and cooldown.
- [ ] Scenario tests for retained values, missing/partial data, and stale-generation writes.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
