# UI-01 validation record

## Scope and prerequisites

Frontend visual scope only. UI-01 adds a CSS design-token layer at the top of `apps/desktop/src/styles.css` (`:root` custom properties for typography, spacing, radius, color palette, and focus ring, with a `prefers-color-scheme: dark` palette override under the existing `color-scheme: light dark`) and maps every shell and shared style rule onto those tokens: skip link, header, language control, synthetic banner, nav buttons, main container, error/empty/note surfaces, charts, tables, and the desktop-integration block. No React tree, className, IPC, generated contract, Rust, persistence, or Tauri capability change was made; `App.tsx` and all screen components are untouched because the existing class hooks covered every styled element.

Prerequisites confirmed before starting: the Phase 1 bilingual shell is merged on `main` (archived P1-08 lineage), the parent milestone PRD `.trellis/tasks/10-25-ui-polish/prd.md` names UI-01 as the first implementable child, and no official Phase 2 provider work is required by this visual track.

No provider call was made (synthetic providers only, unchanged), no credential store or real application data directory was touched, and no production, business, or personal database was connected, not even for reads.

Activation: `python3 .trellis/scripts/task.py start ui-01-tokens-shell` with `TRELLIS_CONTEXT_ID=pi-ui-01-1792895860` after `task.py validate ui-01-tokens-shell` passed (`implement.jsonl` 6 entries, `check.jsonl` 5 entries). Status flipped `planning → in_progress`.

Environment: macOS arm64, Node per `apps/desktop/package.json` engines (`>=22.12.0`), npm 11.

## Validation status

All required local validation passed in this worktree. Commands run in `apps/desktop` unless noted, in order:

- `npm ci` — passed (node_modules was missing; clean install from lockfile).
- `npm run lint` — passed: `ESLint: No issues found`.
- `npm run i18n:check` — passed: `Catalog parity OK: 217 keys, placeholders matching in both catalogs`.
- `npm test` — passed: Vitest 5.0.5, 8 test files, 82 tests, 0 failed.
- `npm run typecheck` — passed: `tsc --noEmit` clean.
- `npm run build` — passed: `tsc --noEmit && vite build`, 44 modules, `dist/assets/index-*.css` 6.36 kB.
- `git diff --check` — passed with no whitespace or conflict-marker findings.

## Acceptance evidence

| PRD criterion | Evidence |
| --- | --- |
| Token variables cover colors, spacing, type scale, radius, and focus; shell rules consume tokens | `apps/desktop/src/styles.css` `:root` now defines `--font-family-*`, `--font-size-xs/sm/md/lg/xl`, `--font-weight-*`, `--space-1..6`, `--radius-sm/md`, the `--color-*` palette (surface, text, text-muted, border, accent + hover/subtle/contrast, warning + surface, danger + surface), and `--focus-ring-width`/`--focus-ring-offset`/`--color-focus-ring`. Every previous one-off value (`#2563eb`, `#8888`, `#b45309`, `#b91c1c`, `#595959`, `#d7dbe0`, `#5b6470`, `#ffffff`, `#a4262c`, literal rem paddings/gaps) is replaced by a token reference in header, nav, banner, and all shared rules; `grep -nE '#[0-9a-fA-F]{3,8}\b' src/styles.css` matches only inside the light `:root` block and the dark `@media` token block. |
| Light and dark `color-scheme` both readable | Light palette is the `:root` default; `@media (prefers-color-scheme: dark)` overrides only the palette (accent shifts to `#7aa5ff`, warning to `#f0a44e`, danger to `#f0958f`, muted text to `#a8b0bb`, surfaces to `#17191d`/`#212429`). This also fixes two real dark-scheme defects in the old sheet: `.desktop-integration-action` had a hardcoded `#ffffff` background and `.desktop-integration` a light-only `#d7dbe0` top border; both now use tokens. Muted text (`--color-text-muted`) replaces fixed `#595959`, which was unreadable on dark surfaces. |
| Synthetic/demo banner unmistakably non-production | `.synthetic-banner` keeps `role="status"` in `App.tsx` (untouched) and is now louder: filled `--color-warning-surface` background, 1px `--color-warning` border, semibold weight, slight letter-spacing, in both schemes. |
| Keyboard focus, skip link, `aria-current` intact | `:focus-visible` keeps a 3px outline with 2px offset, now via focus tokens; `.skip-link` keeps its off-screen/`focus` reveal behavior; `aria-current="page"` nav buttons keep distinct styling (accent border/text plus `--color-accent-subtle` fill). `npm test` (82 tests, including `App.test.tsx` shell tests for skip link, language switching, and nav selection) passed with no selector changes. |
| Language switching preserves current screen; catalog parity | No TSX or i18n file changed; `npm run i18n:check` reports 217 keys with matching placeholders in both catalogs, and the shell tests covering locale switching over screen state all pass. |
| Desktop frontend checks pass | All commands listed above passed on this commit; `git diff --check` clean. |

## Notes for UI-02/UI-03

Screen content layouts (Overview/Statistics/Connections/Settings) were not restructured. Shared rules those screens already consume (`.error`, `.empty`, `.note`, charts, tables, `dt`/`dd`) now draw from tokens, so UI-02/UI-03 should reuse `--color-*`, `--space-*`, and `--font-size-*` rather than introduce new one-off values.
