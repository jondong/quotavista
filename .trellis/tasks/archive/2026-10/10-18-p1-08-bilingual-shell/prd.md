# P1-08: Build the bilingual desktop navigation and display foundations

## Goal

Provide a usable English and Simplified Chinese application shell for all prototype screens.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-02](../10-18-p1-02-contracts/prd.md)

Acceptance mapping: A11, A24, A25. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Create Overview, Statistics, Connections, and Settings navigation with keyboard access, visible focus, loading/empty/error foundations, and an explicit demo indicator.
- Bundle en and zh-CN catalogs with parity checking and English fallback. Implement first-launch system-locale selection and a preference interface ready for persisted settings.
- Centralize locale-aware number/date display and preserve exact values, native currency, and independent reporting zone. Choose UI/chart dependencies only as required by prototype views.

## Acceptance criteria

- [x] Both locales cover navigation, controls, error/status text, accessible labels, and chart descriptions; a parity check catches missing required keys. (Evidence: `npm run i18n:check` — 58-key parity en/zh-CN; Vitest asserts localized chart description, error, empty, and banner text.)
- [x] Explicit Simplified Chinese system locales choose zh-CN; other locales choose en. A user override takes priority, with persistence completed through P1-10. (Evidence: Vitest locale-detection and stored-override tests; override currently persists via renderer-local storage, migration to the versioned setting lands in P1-10.)
- [x] Switching language preserves current screen, filters, and nonsecret input, without changing reporting zone/currency or initiating provider calls. (Evidence: Vitest switching test keeps Statistics filter and Connections alias draft; reporting zone/currency are fixed constants; only the `synthetic_status` command is invoked.)
- [x] Window resizing and keyboard navigation keep primary flows usable; all demo content is identifiable as synthetic. (Evidence: responsive layout rules and `:focus-visible` styling in `styles.css`; native focusable controls with labels and `aria-current` navigation; synthetic banner asserted in Vitest. Native-WebView keyboard evidence remains pending in docs/traceability.md.)

## Out of scope

No final branding, extra locales, website, independent frontend provider calls, or Windows distribution.

## Planning status

Implemented on the P1-08 branch with prototype-level evidence recorded above and in docs/traceability.md; runtime persistence moves to P1-10 and native-WebView interaction evidence remains pending.
