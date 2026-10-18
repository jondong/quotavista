# P1-10 design

## Approach and boundaries

Model operations as backend state with safe progress and retry eligibility. The UI reflects operation status rather than optimistically reporting success. For the Phase 1 no-backup runtime, removal only acknowledges cleanup once all existing artifacts are handled; later managed-backup support must join this same boundary.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-07](../10-18-p1-07-ipc/prd.md), [P1-08](../10-18-p1-08-bilingual-shell/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No real Kimi/OpenRouter login, payment management, multiple simultaneous accounts per provider, full managed restore, or alert-episode engine.
