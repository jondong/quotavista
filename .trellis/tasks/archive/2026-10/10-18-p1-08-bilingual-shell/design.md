# P1-08 design

## Approach and boundaries

Use a single locale service and structured translation catalogs. Locale is a display concern, not an accounting or date-boundary selector. Backend-derived data flows through typed props; do not add financial aggregation to components.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-02](../10-18-p1-02-contracts/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No final branding, extra locales, website, independent frontend provider calls, or Windows distribution.
