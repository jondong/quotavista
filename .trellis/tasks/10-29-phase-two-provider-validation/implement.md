# Phase 2 parent execution plan

## Entry checks

- [x] Phase 2 plan written (`docs/phase-two-plan.md`).
- [x] Trellis children P2-00 through P2-05 created under this parent.
- [ ] Each child archived only after its own acceptance passes.
- [ ] Parent exit gate reviewed against live reconciliation evidence.

## Work sequence

Follow the dependency order in [phase-two-plan.md](../../../docs/phase-two-plan.md). Prefer P2-02 as the first code slice; fold minimal fixtures into P2-02 when P2-01 would block.

## Recovery and completion

Keep the parent open until every child criterion and the exit gate pass. Phase 1 parent stays incomplete independently.
