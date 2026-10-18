# P1-04 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-03](../10-18-p1-03-sqlite/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Define journal records and operation states using opaque IDs only.
- [ ] Implement durable writes, recovery reconciliation, and transactional generation checks.
- [ ] Add interruption/race tests with fake stores and manifests; document platform durability constraints.

## Validation and review

- [ ] Fault-injection tests at operation boundaries, stale-result race tests, corrupt-journal checks, and manifest admission tests.
- [ ] Review persisted synthetic artifacts for secret/identity fields.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
