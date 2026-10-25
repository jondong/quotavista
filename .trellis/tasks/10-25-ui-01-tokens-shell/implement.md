# UI-01 execution plan

## Entry checks

- [x] Read this task's PRD, design, parent PRD, and `.trellis/spec/frontend/` guidance.
- [x] Confirm no official Phase 2 provider work is required here.
- [x] Review existing `apps/desktop/src/styles.css` and `App.tsx` shell structure.

## Work sequence

- [x] Add token custom properties (color, space, type, radius, focus).
- [x] Restyle header, nav, language control, synthetic banner, and main frame using tokens; verify light/dark readability.
- [x] Keep a11y focus/skip-link behaviour; run desktop frontend checks listed in the PRD.
- [x] Commit locally on `jondong/ui-polish-tokens-shell` with an English message. Do not push or open a PR until told.

## Validation and review

- [x] `npm run lint`, `i18n:check`, `test`, `typecheck`, `build` in `apps/desktop`.
- [x] `git diff --check`.
- [x] Record evidence against each PRD criterion; leave UI-02/UI-03 untouched except shared tokens they will reuse.
