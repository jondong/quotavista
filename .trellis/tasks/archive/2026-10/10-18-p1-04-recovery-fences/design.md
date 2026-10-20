# P1-04 design

## Approach and boundaries

External journal state survives database restore. Use a small filesystem abstraction for failure injection and same-directory atomic replacement plus flush/directory sync where supported. Database projections never replace external safety state. This task establishes restore admission, not a full backup product.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-03](../10-18-p1-03-sqlite/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No real provider identity verification, complete credential UI, full managed backup creation/purge/restore, or release deletion certification.
