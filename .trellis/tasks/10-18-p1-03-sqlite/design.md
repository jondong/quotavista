# P1-03 design

## Approach and boundaries

SQLx and SQLite implement core storage ports. Use temporary databases for tests and the OS data directory at runtime. Keep the published revision pointer and partition replacement transactional. Persist schema needed for later features without claiming complete retention, backfill, alerts, or managed recovery.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-02](../10-18-p1-02-contracts/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No provider pagination/backfill, full 90-day retention, managed backup/restore implementation, or complete usage analytics.
