# UI polish milestone design

Ship visual improvements in three reviewable children. UI-01 establishes CSS custom-property tokens and shell chrome so later children reuse one token surface. Prefer refining `apps/desktop/src/styles.css` and light shell markup/class hooks over introducing Tailwind, shadcn, MUI, or similar.

Keep React screen logic, IPC clients, i18n catalogs (except copy that must stay parity-safe), and Rust backends untouched unless a typo blocks styling hooks. Rollback is ordinary task-branch revert.
