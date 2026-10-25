# UI-02 validation evidence

Validation date: 2026-10-25. Branch `jondong/ui-polish-four-screens`, base `dd80259` (main, includes merged UI-01). All commands run in `apps/desktop` with `node_modules` installed via `npm ci`.

## Acceptance criteria

### 1. Each of the four screens uses shared tokens for surfaces, borders, and spacing

Every new rule in the UI-02 block appended to `apps/desktop/src/styles.css` (after the desktop-integration rules) references only UI-01 custom properties (`--color-*`, `--space-*`, `font-size/weight-*`, `--radius-*`); no new color literals were introduced (the only literal in the block is `none` on `border-bottom`).

- Screen roots render one card: `OverviewScreen.tsx` keeps `section.panel`; `StatisticsScreen.tsx` keeps `section.panel`; `ConnectionsScreen.tsx` root and both early-return frames now use `className="panel"`; `SettingsScreen.tsx` root and loading frame now use `className="panel"`. `.panel` = `--color-surface-raised` background, `--color-border` border, `--radius-md`, `--space-4`/`--space-5` padding.
- Nested groups share one rhythm: `section.subsection` / `form.subsection` (hairline `--color-border` top, `--space-4` margins) on Overview summary + scope sections, Statistics total/lane/excluded/plans sections, Connections rotate form / lastError block / operation status / add form, Settings language/reporting/polling/thresholds/prototype groups.
- Overview metrics: `.metric-row` two-column grid (`minmax(10rem, 1fr)` / `minmax(0, 2fr)`), `.metric-timestamps` muted `--font-size-xs` row, `.freshness-badge` `--color-warning`.
- Statistics: `.statistics-controls` toolbar (surface, `--radius-md`), `.panel .statistics-total` hero (`--font-size-xl`, `--font-weight-semibold`), `.chart-empty` dashed frame mirroring `.empty`.
- Tables: `th` background `--color-surface` + `--font-size-sm`, `td` `--font-size-sm`; both tables (`.plans-table` in StatisticsScreen, connection table in ConnectionsScreen) wrapped in `<div className="table-wrap">` (overflow-x).
- Controls: `.panel input/select/button` chrome scoped to panels so `.nav-button`, header language select, and `.desktop-integration-action` keep their UI-01 styling; `.button-primary` submit accent (`--color-accent` + `--color-accent-contrast`, verified ≥ 4.5:1 both schemes).
- Settings: save button + saved status share `.form-actions` row; submit is `.button-primary`.

### 2. Loading/empty/error foundations remain bilingual and accessible

No i18n keys, catalog entries, `role` attributes, `aria-*` attributes, or role/level structure changed. Loading frames render `t("common.loading")` (`.note`), error frames `role="alert"` + `.error` (Overview, Statistics, Connections, Settings), empty frames `t("connections.list.empty")` with `.empty` and `.chart-empty`. All text remains sourced from the bilingual catalog; en/zh-CN parity checked by `npm run i18n:check` (below). Connections early returns gained a `panel` wrapper + `h2` heading — heading level and text unchanged, `role` on the message paragraphs unchanged.

### 3. No IPC or contract changes; desktop frontend checks green

`git diff` touches only `apps/desktop/src/styles.css`, the four `apps/desktop/src/screens/*.tsx` files (className/wrapper only), and Trellis task artifacts. No `src-tauri`, `src/commands`, `src/ipc`, or catalog changes. All checks green:

| Check | Command | Result |
| --- | --- | --- |
| Dependencies | `npm ci` | clean install, exit 0 |
| Lint | `npm run lint` | pass, no findings (eslint .) |
| Catalog parity | `npm run i18n:check` | `Catalog parity OK: 217 keys, placeholders matching in both catalogs` |
| Tests | `npm test` | 8 test files passed, 82 tests passed (App.test.tsx, i18n/i18n.test.ts, ipc.test.ts, components/DesktopIntegration.test.tsx, screens/{Overview,Statistics,Connections,Settings}Screen.test.tsx) |
| Types | `npm run typecheck` | pass (`tsc --noEmit`, no errors) |
| Build | `npm run build` | pass (`vite build`, 44 modules, `✓ built in 267ms`) |
| Whitespace | `git diff --check` | clean |

Tests were left untouched: they query by role/text (e.g. `getByRole("button")`, `getByText`), and the only class-based assertion, `StatisticsScreen.test.tsx` `document.querySelector(".bar-partial")`, targets chart classes that this task did not modify.

## Scope boundaries respected

- MetricValue, provenance rows, and Connections form deep readability untouched (UI-03).
- Shell (skip-link, nav, synthetic banner, desktop-integration) untouched; `main.container` markup unchanged.
- No UI library added; no Tailwind/shadcn/MUI/Chakra; CSS only.
- No Rust, persistence, or Tauri capability changes; synthetic providers only.
