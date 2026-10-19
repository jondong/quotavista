# P1-01 design

## Approach and boundaries

Use a Cargo workspace and a static Vite frontend. The desktop package composes the core, providers, and infrastructure crates. Choose package names that avoid Rust core-name ambiguity; preserve the reviewed filesystem boundaries. Keep the prototype unsigned and exclude Trellis tooling from application assets.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: None; first implementation task. Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No complete screens, provider integration, signing, installers, auto-updates, or minimum macOS support claim.
