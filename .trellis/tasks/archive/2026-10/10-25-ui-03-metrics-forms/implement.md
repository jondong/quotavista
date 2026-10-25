# UI-03 execution plan

## Entry checks

- [x] Read this task's PRD, design, parent PRD, and `.trellis/spec/frontend/` guidance.
- [x] Confirm UI-01 and UI-02 are merged on `main` (branch based on `1de947a`, which carries tokens, shell polish, and the four-screen layout).
- [x] Review `apps/desktop/src/styles.css` token layer and screen blocks, `components/MetricValue.tsx`, `screens/OverviewScreen.tsx`, `screens/StatisticsScreen.tsx`, and `screens/ConnectionsScreen.tsx`.

## Work sequence

- [x] Metric values vs. availability states in `components/MetricValue.tsx`: wrap formatted values in a `.metric-value` span (tabular numerals, semibold) and style `.metric-state` / `.reason-text` as muted status text in `styles.css`, so unknown/unsupported never reads as a number. No behavior, reason-code, or text changes.
- [x] Provenance / freshness / coverage cues: extend `.freshness-badge` into a warning pill (AA contrast verified against `--color-warning-surface` in both schemes); subordinate the range/provisional provenance lines in `OverviewScreen.tsx` and `StatisticsScreen.tsx` with the shared `.note` treatment so the hero values keep visual priority. Text content unchanged.
- [x] Connections form readability in `screens/ConnectionsScreen.tsx`: add `.form-field` wrappers for label/input/hint groups (block labels, consistent spacing, bounded control width), reuse `.form-actions` for submit + note rows, give the inline rename editor a flex row, give the last-error block the shared `.error` surface (keeps `role="alert"`), and treat the operation status block as a status card with a `data-state` hook for the failed state. No validation-rule, secret-handling, or IPC changes; labels, ids, and roles unchanged.
- [x] CSS only in `apps/desktop/src/styles.css` as a new UI-03 block; consume UI-01 tokens only, no new color literals, no UI library.
- [x] Keep screen tests green; do not weaken assertions (tests query by role/label/text, all unchanged).
- [x] Commit locally on `jondong/ui-polish-metrics-forms` with an English message. Do not push, open a PR, merge, or archive.

## Validation and review

- [x] `npm ci` (only if node_modules missing/outdated), `npm run lint`, `npm run i18n:check`, `npm test`, `npm run typecheck`, `npm run build` in `apps/desktop`.
- [x] `git diff --check`.
- [x] Record evidence against each PRD criterion in `validation.md`; leave the task `in_progress`.
