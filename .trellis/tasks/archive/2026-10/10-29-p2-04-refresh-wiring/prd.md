# P2-04: Refresh wiring for live adapters

## Goal

Connect live OpenRouter and/or Kimi adapters to the existing refresh coordinator and connection management so per-connection refresh, merging, auth handling, and last-successful-value preservation work with real adapters.

## Baseline and prerequisites

Advisory dependencies: [P2-02](../10-29-p2-02-openrouter-adapter/prd.md) and [P2-03](../10-29-p2-03-kimi-adapter/prd.md). Start when at least one live adapter surface is reviewable.

Acceptance mapping: A06, A07 contributions with live adapters.

## Scope and deliverables

- Adapter selection by connection provider type.
- Preserve Phase 1 scheduling invariants (concurrency two, coalesce, cooldown, fences).
- Auth failures typed and isolated per connection/dataset.

## Acceptance criteria

- [ ] Live adapter jobs participate in refresh without contacting providers from demo mode.
- [ ] Failure retention and isolation tests pass with live adapter fakes/transport injectors.
- [ ] No regression in synthetic path.

## Out of scope

Credential rotation UI (P2-05); history backfill (Phase 3).
