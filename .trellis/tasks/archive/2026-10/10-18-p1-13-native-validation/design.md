# P1-13 design

## Approach and boundaries

Use the same prototype revision on each native target and disposable synthetic data. A VM/native desktop session is acceptable if its exact environment is recorded and actual desktop capabilities are exercised; a headless build is insufficient.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-12](../archive/2026-10/10-18-p1-12-integration-validation/prd.md). Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No Windows support, live-provider access, installers/notarization release acceptance, or claim that build success proves desktop usability.
