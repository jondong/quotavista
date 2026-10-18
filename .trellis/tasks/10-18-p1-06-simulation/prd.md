# P1-06: Add deterministic simulated providers and refresh coordination

## Goal

Exercise realistic success and failure states without contacting providers.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-04](../10-18-p1-04-recovery-fences/prd.md)

Acceptance mapping: A05, A06, A07, A16, A17, A18, A19, A22. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Implement clearly synthetic Kimi shared-pool and OpenRouter prepaid/key-budget capability fixtures through provider ports, with no real endpoint/auth assumptions.
- Add an injectable-clock coordinator with one job per connection, global concurrency two, cancellation/generation checks, manual coalescing, and persisted cooldown boundaries.
- Add deterministic current-state/history scenarios for zero, unknown, stale upstream, partial permission, missing tokens, UTC buckets, failed refresh, and incomplete replacement.

## Acceptance criteria

- [ ] Demo mode cannot issue provider requests; fixtures contain synthetic identities and explicit simulated provenance.
- [ ] Refresh failure for one dataset/provider retains committed values and does not clear another dataset error.
- [ ] Automatic refresh Off produces no implicit launch/wake/recovery query. Manual requests coalesce and respect cooldown; cancellation releases concurrency slots.
- [ ] Late synthetic responses are fenced after disable/delete; partial history stays separate from published totals and cannot claim completeness.

## Out of scope

No live requests, real retry tuning, real pagination/backfill, or provider capability research.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
