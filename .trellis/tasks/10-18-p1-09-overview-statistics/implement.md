# P1-09 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: [P1-07](../10-18-p1-07-ipc/prd.md), [P1-08](../10-18-p1-08-bilingual-shell/prd.md)
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Seed deterministic published facts with known expected results and missing/UTC-only scenarios.
- [ ] Implement bounded fixture-backed query use cases and overview/statistics components.
- [ ] Verify both locales, empty/failure states, boundaries, and query cancellation without erasing the last display.

## Validation and review

- [ ] Known-result Rust aggregation and range tests; frontend type/build/catalog checks.
- [ ] Renderer interaction checks for presets/custom ranges, exclusions, stale data, UTC lanes, and language switching.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
