# P1-12 design

## Approach and boundaries

Use the task-specific tests already created rather than duplicating them. Add only missing integration coverage at boundaries. Keep reusable performance fixtures synthetic and benchmark output separate from application state.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-09](../archive/2026-10/10-18-p1-09-overview-statistics/prd.md), [P1-10](../archive/2026-10/10-18-p1-10-connections-settings/prd.md), [P1-11](../archive/2026-10/10-18-p1-11-desktop-lifecycle/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No real-account reconciliation, final release certification, provider capability research, or closure of G04 based on automated checks.
