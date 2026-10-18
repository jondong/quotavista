# P1-07 design

## Approach and boundaries

Keep commands thin: validate, map into application use cases, map safe results. Domain objects are not transport objects. Main-window permissions are explicit; bundle local frontend assets and avoid privileged remote windows.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-05](../10-18-p1-05-secret-stores/prd.md), [P1-06](../10-18-p1-06-simulation/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No arbitrary diagnostic endpoints, remote login window, cloud API, or complete real-provider operations.
