# P1-11 design

## Approach and boundaries

Use small Tauri/OS adapters composed behind application lifecycle ports. Tray availability is a capability, not an assumption about GNOME. Keep notification template keys separate from future persistent alert episodes.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-07](../10-18-p1-07-ipc/prd.md), [P1-08](../10-18-p1-08-bilingual-shell/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No background service, complete threshold/expiration alert engine, installer signing, or GNOME compatibility claim without a native run.
