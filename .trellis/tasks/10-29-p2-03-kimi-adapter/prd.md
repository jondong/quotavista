# P2-03: Kimi shared-membership adapter

## Goal

Implement a Kimi shared-membership quota adapter reconciled against the official membership page for the same period and scope.

## Baseline and prerequisites

Dependencies: [P2-00](../10-29-p2-00-plan/prd.md). Soft on P2-01.

Acceptance mapping: A01–A03 primary; A02 scope rules mandatory.

## Scope and deliverables

- Adapter for shared membership pool used/remaining and refresh time when exposed.
- Explicit refusal to substitute API wallet or independent Code limits for membership quota.
- Sanitized fixtures; live auth only from local secret store / session memory.

## Acceptance criteria

- [ ] Fixture tests cover shared-pool metrics and unsupported substitutes.
- [ ] Live reconciliation notes vs official membership page when credentials available.
- [ ] No cookies, raw headers, or identifying account data in repository fixtures.

## Out of scope

OpenRouter (P2-02); Extra Usage P1 features beyond explicit status if exposed.
