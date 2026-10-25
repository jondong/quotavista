# UI-01 execution plan

## Entry checks

- [ ] Read this task's PRD, design, parent PRD, and `.trellis/spec/frontend/` guidance.
- [ ] Confirm no official Phase 2 provider work is required here.
- [ ] Review existing `apps/desktop/src/styles.css` and `App.tsx` shell structure.

## Work sequence

- [ ] Add token custom properties (color, space, type, radius, focus).
- [ ] Restyle header, nav, language control, synthetic banner, and main frame using tokens; verify light/dark readability.
- [ ] Keep a11y focus/skip-link behaviour; run desktop frontend checks listed in the PRD.
- [ ] Commit locally on `jondong/ui-01-tokens-shell` with an English message. Do not push or open a PR until told.

## Validation and review

- [ ] `npm run lint`, `i18n:check`, `test`, `typecheck`, `build` in `apps/desktop`.
- [ ] `git diff --check`.
- [ ] Record evidence against each PRD criterion; leave UI-02/UI-03 untouched except shared tokens they will reuse.
