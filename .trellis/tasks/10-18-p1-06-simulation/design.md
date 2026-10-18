# P1-06 design

## Approach and boundaries

Adapters return structured capabilities, current metrics, and bounded candidate pages. The application coordinator owns scheduling and persistence. Use controllable fake time and explicit scenario selection rather than random failures or frontend polling.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-04](../10-18-p1-04-recovery-fences/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No live requests, real retry tuning, real pagination/backfill, or provider capability research.
