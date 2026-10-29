# P2-05: Credential rotation

## Goal

Validate same-scope credential rotation and explicit identity replacement, including failures between native secret-store and database steps, without inference calls.

## Baseline and prerequisites

Dependencies: [P2-04](../10-29-p2-04-refresh-wiring/prd.md).

Acceptance mapping: supports A04 scope honesty and connection credential lifecycle; complements Phase 4 full flows.

## Scope and deliverables

- Same-account / same-scope key rotation path.
- Explicit identity replacement path when scope changes.
- Failure injection between secret-store write and DB commit (and reverse) with safe recovery.
- Response/retry limits verified without inference traffic.

## Acceptance criteria

- [ ] Rotation keeps connection identity when scope matches; replacement requires explicit user action when identity changes.
- [ ] Mid-flight failures leave no plaintext secrets on disk and no half-applied trusted state without a visible recovery path.
- [ ] Tests do not call provider inference endpoints.

## Out of scope

Full Phase 4 connection UX polish; multi-provider batch rotates.
