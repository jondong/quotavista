# P1-09 design

## Approach and boundaries

Implement the minimum backend aggregation necessary for seeded compatible facts and these observable prototype states. Reuse P1-02 numeric/time helpers and P1-03 published facts. Charts consume backend-prepared values; provider reconciliation and full backfill remain Phase 3.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-07](../archive/2026-10/10-18-p1-07-ipc/prd.md), [P1-08](../archive/2026-10/10-18-p1-08-bilingual-shell/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No live usage totals, estimates from balances/quota differences, provider completeness claim, full retention/backfill engine, or final charts polish.
