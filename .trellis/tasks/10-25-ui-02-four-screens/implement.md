# UI-02 execution plan

## Entry checks

- [x] Read this task's PRD, design, parent PRD, and `.trellis/spec/frontend/` guidance.
- [x] Confirm UI-01 is merged on `main` (branch based on `dd80259`, which carries the token layer and shell polish).
- [x] Review `apps/desktop/src/styles.css` token layer and the four screen components.

## Work sequence

- [x] Add screen-content layout rules to `apps/desktop/src/styles.css` consuming only UI-01 tokens: panel/surface treatment for `section` wrappers, section heading rhythm, metric rows, statistics controls/lanes, connections table + forms, settings field groups.
- [x] Apply light className / wrapper adjustments in `OverviewScreen.tsx`, `StatisticsScreen.tsx`, `ConnectionsScreen.tsx`, `SettingsScreen.tsx` where the existing markup lacks a hook for shared rules. No behaviour or IPC payload changes; loading/empty/error frames stay bilingual and accessible.
- [x] Keep screen tests green; update selectors only where a class hook was added (never weaken assertions).
- [x] Commit locally on `jondong/ui-polish-four-screens` with an English message. Do not push, open a PR, merge, or archive.

## Validation and review

- [x] `npm ci` (only if node_modules missing/outdated), `npm run lint`, `npm run i18n:check`, `npm test`, `npm run typecheck`, `npm run build` in `apps/desktop`.
- [x] `git diff --check`.
- [x] Record evidence against each PRD criterion in `validation.md`; leave the task `in_progress`.
