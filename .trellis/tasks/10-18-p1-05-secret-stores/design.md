# P1-05 design

## Approach and boundaries

Native adapters implement the same application port as a failure-capable test store. Candidate validation is synthetic in Phase 1 and cannot prove Kimi/OpenRouter access. Use opaque versioned references and execute store calls on blocking workers.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-04](../10-18-p1-04-recovery-fences/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No browser-cookie import, real credentials, provider authorization, or automatic downgrade to session-only storage.
