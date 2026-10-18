# P1-02 design

## Approach and boundaries

Domain primitives stay serialization-independent. Place transport types at a dedicated boundary with explicit mappings and one contract-generation strategy. Return exact integer/decimal strings; carry native units and source bucket boundaries through every layer.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-01](../10-18-p1-01-workspace/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No live endpoint schema, universal token availability assumption, complete statistics engine, or provider authentication research.
