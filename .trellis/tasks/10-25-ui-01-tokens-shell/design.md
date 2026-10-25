# UI-01 design

## Approach and boundaries

Add a small token layer at the top of `apps/desktop/src/styles.css` (`:root` custom properties; optional `@media (prefers-color-scheme: dark)` or `color-scheme`-aware pairs). Map existing shell classes (`.app-header`, nav, `.synthetic-banner`, main content) onto those tokens. Prefer class-level polish over restructuring React trees. Touch `App.tsx` or shell components only when a className/wrapper is required for layout.

Do not introduce npm UI libraries. Do not change `ipc.ts`, `generated/contracts.ts`, or Tauri/Rust code.

## Dependencies and contracts

Depends on the merged Phase 1 bilingual shell. Display contracts and synthetic provenance banners must remain honest. Locale stays a display concern.

## Compatibility and rollback

Keep en/zh-CN behaviour, independent reporting zone/currency, and existing Vitest shell tests green (update selectors only if class hooks change, never weaken assertions). Revert the task branch as a unit if checks fail. Never delete a real application data directory.
