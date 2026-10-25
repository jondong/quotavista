# UI-01: Design tokens and shell chrome polish

## Goal

Introduce CSS design tokens and polish the application shell (header, navigation, synthetic banner, dark/light consistency) so the prototype looks intentional without adding a UI component library.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), parent [UI polish](../10-25-ui-polish/prd.md). No Trellis predecessor; builds on merged Phase 1 bilingual shell (archived P1-08).

## Scope and deliverables

- Define CSS custom properties (color, spacing, typography, radius, border, focus) on `:root` with coherent `color-scheme: light dark` behaviour.
- Restyle shell chrome in `apps/desktop/src/styles.css` and minimal class hooks in `App.tsx` / shared shell components as needed: header, nav tabs, language control, synthetic/demo banner, main content frame, skip-link/focus-visible remain accessible.
- Keep default approach: **improve bare CSS tokens** — do not add Tailwind, shadcn, MUI, Chakra, or similar unless this PRD is explicitly revised.
- Preserve en/zh-CN switching, screen selection state, and synthetic labelling.

## Acceptance criteria

- [x] Token variables cover colors, spacing, type scale, radius, and focus; shell rules consume tokens rather than one-off hex/spacing litter for header/nav/banner.
- [x] Light and dark `color-scheme` both remain readable for shell chrome (no white-on-white / black-on-black regressions in either scheme).
- [x] Synthetic/demo banner stays highly visible and clearly non-production.
- [x] Keyboard focus (`:focus-visible`), skip link, and `aria-current` navigation behaviour remain intact.
- [x] Language switching still preserves the current screen; catalog parity check still passes.
- [x] Desktop frontend checks pass: in `apps/desktop`, `npm ci` (if needed), `npm run lint`, `npm run i18n:check`, `npm test`, `npm run typecheck`, `npm run build`; also `git diff --check`.

## Out of scope / constraints

- No new UI component library by default.
- No IPC / generated contract changes; no Rust domain or persistence changes.
- No real provider calls; synthetic providers only.
- No production or business databases; do not touch `prod/` if present.
- No real Keychain or Secret Service product entries; do not use the real application data directory.
- No deep restyle of Overview/Statistics/Connections/Settings content layouts (UI-02) or MetricValue/form microcopy layout (UI-03) beyond what shared tokens force.
- English task artifacts and commit messages only.

## Planning status

Ready for implementation on branch `jondong/ui-polish-tokens-shell`.
