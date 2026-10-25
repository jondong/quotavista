# UI-03 validation evidence

Validation date: 2026-10-25. Branch `jondong/ui-polish-metrics-forms`, base `1de947a` (main, includes merged UI-01 + UI-02). All commands run in `apps/desktop` with `node_modules` installed via `npm ci`.

## Acceptance criteria

### 1. Metric and provenance presentation uses shared tokens and remains honest about synthetic/partial data

Every new rule in the UI-03 block appended to `apps/desktop/src/styles.css` (after the UI-02 screen-content rules) references only UI-01 custom properties; no new color literals were introduced anywhere in the task (`line-height: 1.4`, `max-width: 24rem`, and `width: 100%` are the only non-token values and carry no color).

- Real values vs. availability states: `components/MetricValue.tsx` now wraps each formatted value in a `.metric-value` span (`font-variant-numeric: tabular-nums`, `--font-weight-semibold`), while unknown/unsupported render through the pre-existing `.metric-state` / `.reason-text` spans, now styled `--color-text-muted` — a state can no longer be mistaken for a number, and a null still never renders as zero (component logic untouched).
- Freshness: `.freshness-badge` (Overview scope rows + timestamps, Statistics plan table) is now a warning pill: `--color-warning` text/border on `--color-warning-surface`. Contrast computed from the UI-01 palette: light `#b45309` on `#fdf1e3` ≈ 4.52:1 (AA pass for text), dark `#f0a44e` on `#35260f` ≈ 7.0:1.
- Provenance subordination: the range/provisional lines in `OverviewScreen.tsx` (`overview.summary.range` + `overview.summary.provisional`) and `StatisticsScreen.tsx` (`statistics.range.summary`, `statistics.range.asOf` + `statistics.range.provisional`) now use the shared `.note` treatment (muted, `--font-size-sm`), keeping the hero `.statistics-total` and metric rows visually primary while the honesty cues stay present as text. Coverage rows in the lane figcaptions, the `excluded_scopes` list, and the synthetic banner are untouched and unchanged.
- Muted text contrast (used by `.note`, `.metric-state`, `.reason-text`): `--color-text-muted` on `--color-surface-raised` ≈ 6.2:1 light / 7.1:1 dark (AA pass).

### 2. Connections form remains keyboard-accessible with visible focus and bilingual labels

- Field grouping: every label/control/hint group in `ConnectionsScreen.tsx` (credential kind, secret, storage mode in the shared `CredentialFields`, plus provider and alias in the add form) is wrapped in `.form-field`: block labels (`--font-size-sm`, `--font-weight-semibold`), `--space-3` group spacing, controls bounded at `max-width: 24rem`, hints pinned under their control. Submit + note pairs in the add and rotate forms share the existing `.form-actions` row; the inline rename editor is a `.rename-editor` flex row inside the table cell.
- Error/status readability: the last-error block keeps `role="alert"` and now also carries the shared `.error` danger surface (heading + message inside the same alert box); `OperationStatus` renders as an `.operation-status` card (`--focus-ring-width` accent left edge, `data-state` hook turning the edge `--color-danger` on failure). Status is still communicated with text first; color is reinforcement only.
- Accessibility unchanged: no i18n key, label text, `id`/`htmlFor` pairing, `role`, or `aria-*` attribute was added, removed, or reworded; the global `:focus-visible` ring still covers every control. en/zh-CN parity verified by `npm run i18n:check` (below). All status/label text remains catalog-sourced.

### 3. No credential plaintext paths; session-only/native store behaviour untouched

The diff touches no credential logic: `CredentialFields` still clears the secret on submission and on language switch, `submitAdd`/`submitRotate` still lift the secret out of state before awaiting, storage-mode options are unchanged, and no IPC payload, validation rule, or `src-tauri` file was modified (`git diff --stat` shows only `styles.css`, `components/MetricValue.tsx`, three screen files, and Trellis artifacts).

### 4. Desktop frontend checks green

| Check | Command | Result |
| --- | --- | --- |
| Dependencies | `npm ci` | clean install, exit 0 |
| Lint | `npm run lint` | pass, `ESLint: No issues found` |
| Catalog parity | `npm run i18n:check` | `Catalog parity OK: 217 keys, placeholders matching in both catalogs` |
| Tests | `npm test` | 8 test files passed, 82 tests passed (App.test.tsx, i18n/i18n.test.ts, ipc.test.ts, components/DesktopIntegration.test.tsx, screens/{Overview,Statistics,Connections,Settings}Screen.test.tsx) |
| Types | `npm run typecheck` | pass (`tsc --noEmit`, no errors) |
| Build | `npm run build` | pass (`vite build`, `✓ built in 282ms`) |
| Whitespace | `git diff --check` | clean |

Tests were left untouched and still query by role/label/text; the one class-based assertion (`StatisticsScreen.test.tsx` `.bar-partial`) targets chart classes this task did not modify.

## Scope boundaries respected

- Shell (skip-link, nav, synthetic banner, desktop-integration) untouched; the synthetic banner rule and text are unchanged.
- No reopening of UI-01 tokens or UI-02 layout density: the only shared-class edits are the `.freshness-badge` pill upgrade and reuse of `.note` / `.form-actions` / `.error`.
- No UI library added (no Tailwind/shadcn/MUI/Chakra); CSS plus light className/markup wrappers only.
- No Rust, persistence, Tauri capability, or generated-contract changes; synthetic providers only; no real Keychain/Secret Service entries or application data touched.
