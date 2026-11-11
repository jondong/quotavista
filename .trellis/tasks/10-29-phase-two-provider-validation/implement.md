# Phase 2 parent execution plan

## Entry checks

- [x] Phase 2 plan written (`docs/phase-two-plan.md`).
- [x] Trellis children P2-00 through P2-05 created under this parent; P2-06 (live-smoke evidence and auth guidance corrections) added 2026-11-11.
- [ ] Each child archived only after its own acceptance passes.
- [ ] Parent exit gate reviewed against live reconciliation evidence.

## Work sequence

Follow the dependency order in [phase-two-plan.md](../../../docs/phase-two-plan.md). Prefer P2-02 as the first code slice; fold minimal fixtures into P2-02 when P2-01 would block.

## Recovery and completion

Keep the parent open until every child criterion and the exit gate pass. Phase 1 parent stays incomplete independently. The user-reported 2026-11-10 OpenRouter live smoke recorded by P2-06 is partial adapter evidence only: it printed dataset categories and no figure, so the OpenRouter console reconciliation and the Kimi member-page reconciliation are both still owed and the parent PRD exit-gate checkboxes stay unchecked.
