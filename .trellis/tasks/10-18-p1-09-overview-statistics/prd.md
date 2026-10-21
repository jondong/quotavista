# P1-09: Implement simulated overview and period statistics views

## Goal

Let users inspect clearly simulated remaining resources and period consumption with honest units and coverage.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-07](../archive/2026-10/10-18-p1-07-ipc/prd.md), [P1-08](../archive/2026-10/10-18-p1-08-bilingual-shell/prd.md)

Acceptance mapping: A05, A13, A14, A15, A17, A22, A25. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Display Kimi shared quota and refresh time separately from OpenRouter account Credits/key budget, with source timestamps and independent health/freshness/coverage.
- Implement all reviewed range presets and custom selection, day/week views, recorded-token totals, per-plan native-unit rows, and trends through backend queries over deterministic local fixtures.
- Expose UTC-only lanes, provisional current periods, excluded scopes, and missing/partial/unsupported states in both locales.

## Acceptance criteria

- [x] A fixture with known tokens/money/quota produces exact expected totals and distinct units; cached/reasoning subcounts and account/key representations are not added twice.
- [x] Missing Kimi tokens exclude that scope from Recorded tokens while its native quota consumption remains visible; unavailable history is never a zero line.
- [x] Range filters show explicit dates/zone, enforce 366 days/future-date rules, ignore obsolete responses, and preserve selection during language switching.
- [x] UTC-only buckets remain in an explicit compatible lane and excluded edge buckets leave partial coverage. Current-day results are clipped at as-of and marked provisional.

## Out of scope

No live usage totals, estimates from balances/quota differences, provider completeness claim, full retention/backfill engine, or final charts polish.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Technical choices were checked against current official documentation during implementation. All acceptance criteria are ticked with the evidence recorded in validation.md.
