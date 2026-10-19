# P1-02 execution plan

## Entry checks

- [x] Read the PRD, design, relevant specs, and reviewed baseline.
- [x] Confirm predecessor tasks are completed, including archived records: [P1-01](../archive/2026-10/10-18-p1-01-workspace/prd.md)
- [x] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [x] Define domain primitives and validation rules from architecture sections on data semantics.
- [x] Implement range helpers and representative numeric/time tests.
- [x] Define transport mappings and generate/check TypeScript; add a drift check to the P1-01 workflow.

## Validation and review

- [x] Rust unit tests for extreme values, invalid decimals, time boundaries, and unavailable states.
- [x] Contract generation/check leaves a clean diff; frontend type check passes.
- [x] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [x] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
