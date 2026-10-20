# P1-06 execution plan

## Entry checks

- [x] Read the PRD, design, relevant specs, and reviewed baseline.
- [x] Confirm predecessor tasks are completed, including archived records: [P1-04](../archive/2026-10/10-18-p1-04-recovery-fences/prd.md) — merged via PR #5 (46f8db9) and archived in 2026-10.
- [x] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [x] Add fixture scenarios and a network-disabled simulated adapter. — `crates/providers/src/simulated.rs`: ten scenarios over the two capability fixtures (the eight PRD states plus the review-round no-history and withdrawn-terminal-attestation fixtures); no network dependency anywhere in the crate.
- [x] Implement a bounded coordinator through core ports and repository operations. — `crates/core/src/provider.rs` (port, errors, cooldowns) and `crates/infrastructure/src/refresh.rs` (stepping coordinator, concurrency two, page/time-bounded history).
- [x] Run deterministic scheduling, freshness, cooldown, cancellation, and partial-history tests. — `crates/infrastructure/src/refresh/tests.rs` (8 tests: 7 coordinator fixtures plus the partition-shape guard), `crates/core/src/provider.rs` (2 port/cooldown tests), `crates/providers/src/simulated.rs` (7 scenario tests).
## Validation and review

- [x] Fake-clock tests for manual-only mode, coalescing, concurrency, wake behavior, and cooldown. — `automatic_refresh_off_launches_nothing_and_on_respects_interval`, `manual_requests_coalesce_and_respect_persisted_cooldowns`, `concurrency_is_two_and_cancellation_frees_the_slot`.
- [x] Scenario tests for retained values, missing/partial data, and stale-generation writes. — `failing_dataset_keeps_committed_values_and_errors_isolated`, `late_responses_are_fenced_after_disable_and_deletion`, `incomplete_history_keeps_published_partition_untouched`, plus the eight adapter scenarios in `simulated.rs`.
- [x] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability. — PRD criteria ticked with test-name evidence; `docs/traceability.md` and `.trellis/spec/backend/database-guidelines.md` updated.
- [x] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
