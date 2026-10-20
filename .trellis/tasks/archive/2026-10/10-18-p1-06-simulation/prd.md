# P1-06: Add deterministic simulated providers and refresh coordination

## Goal

Exercise realistic success and failure states without contacting providers.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-04](../archive/2026-10/10-18-p1-04-recovery-fences/prd.md)

Acceptance mapping: A05, A06, A07, A16, A17, A18, A19, A22. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Implement clearly synthetic Kimi shared-pool and OpenRouter prepaid/key-budget capability fixtures through provider ports, with no real endpoint/auth assumptions.
- Add an injectable-clock coordinator with one job per connection, global concurrency two, cancellation/generation checks, manual coalescing, and persisted cooldown boundaries.
- Add deterministic current-state/history scenarios for zero, unknown, stale upstream, partial permission, missing tokens, UTC buckets, failed refresh, and incomplete replacement.

## Acceptance criteria

- [x] Demo mode cannot issue provider requests; fixtures contain synthetic identities and explicit simulated provenance. — `quotavista-providers` has no network dependency; `SyntheticProvider` identities and `Provenance::Synthetic` asserted by `manual_refresh_publishes_history_and_commits_current`.
- [x] Refresh failure for one dataset/provider retains committed values and does not clear another dataset error. — `failing_dataset_keeps_committed_values_and_errors_isolated`.
- [x] Automatic refresh Off produces no implicit launch/wake/recovery query. Manual requests coalesce and respect cooldown; cancellation releases concurrency slots. — `automatic_refresh_off_launches_nothing_and_on_respects_interval`, `manual_requests_coalesce_and_respect_persisted_cooldowns`, `concurrency_is_two_and_cancellation_frees_the_slot`.
- [x] Late synthetic responses are fenced after disable/delete; partial history stays separate from published totals and cannot claim completeness. — `late_responses_are_fenced_after_disable_and_deletion`, `incomplete_history_keeps_published_partition_untouched`.

## Out of scope

No live requests, real retry tuning, real pagination/backfill, or provider capability research.

## Planning status

Implementation complete on this task branch: all acceptance criteria evidenced by deterministic fake-clock tests (see `implement.md`); full workspace validation (fmt, clippy, tests, boundary scripts, contracts check) green. Awaiting review and merge.
