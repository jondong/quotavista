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

- [ ] Both locales cover navigation, controls, error/status text, accessible labels, and chart descriptions; a parity check catches missing required keys.
- [ ] Explicit Simplified Chinese system locales choose zh-CN; other locales choose en. A user override takes priority, with persistence completed through P1-10.
- [ ] Switching language preserves current screen, filters, and nonsecret input, without changing reporting zone/currency or initiating provider calls.
- [ ] Window resizing and keyboard navigation keep primary flows usable; all demo content is identifiable as synthetic.

## Out of scope

No final branding, extra locales, website, independent frontend provider calls, or Windows distribution.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
