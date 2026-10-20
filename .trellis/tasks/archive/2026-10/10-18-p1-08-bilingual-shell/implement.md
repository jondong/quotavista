# P1-08 execution plan

## Entry checks

- [x] Read the PRD, design, relevant specs, and reviewed baseline.
- [x] Confirm predecessor tasks are completed, including archived records: [P1-02](../10-18-p1-02-contracts/prd.md) (PR #2, merge commit 70bf77f; archived under `.trellis/tasks/archive/2026-10/10-18-p1-02-contracts`)
- [x] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [x] Implement navigation and accessible view foundations.
- [x] Implement locale selection/catalogs/formatters and parity command; integrate the CI check.
- [x] Verify both languages and state-preserving switching in the renderer; record source conventions.

## Validation and review

- [x] Frontend type/lint/build and catalog-parity checks. (`npm run i18n:check` 58-key parity; `npm run lint`, `npm run typecheck`, `npm test` (18 passed), `npm run build` all clean; `cargo fmt`/`clippy -D warnings`/`cargo test --workspace`/contracts drift check green.)
- [x] Interaction checks for locale selection, live switching, focus, resizing, and accessible labels. (Vitest renderer tests for selection/switching/state preservation/labels; `:focus-visible` outline, skip link, responsive stacking implemented in `styles.css`; native-WebView runtime evidence remains pending.)
- [x] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability. (PRD criteria ticked with evidence; `docs/traceability.md` P1-08 section, `.trellis/spec/frontend/quality-guidelines.md`, `CONTRIBUTING.md`, `.github/workflows/ci.yml` updated.)
- [x] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
