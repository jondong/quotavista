# UI polish — design tokens and screen visuals

## Goal

Make the bilingual desktop prototype visually coherent and less austere while keeping behaviour, IPC contracts, and synthetic-provider boundaries unchanged. This milestone is a **visual track**, not official Phase 2 provider validation in [development-plan.md](../../../docs/development-plan.md).

## Authoritative baseline

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), and the existing Phase 1 prototype on `main`.

## Scope

- Design tokens and shell chrome (UI-01).
- Four-screen layout polish (UI-02).
- MetricValue / provenance / Connections form readability (UI-03).

## Phase exit acceptance

- [ ] UI-01 lands tokens and shell polish with bilingual shell still working and existing desktop frontend checks green.
- [ ] UI-02 and UI-03 land layout and component readability improvements that consume UI-01 tokens (no parallel ad-hoc color systems).
- [ ] No IPC contract, synthetic-provider, credential-store, or persistence-schema changes.
- [ ] English docs/task artifacts only; en/zh-CN catalog parity preserved.

## Out of scope

Official Phase 2 provider work; real Keychain/Secret Service product use; production or business databases; new UI component libraries unless a child PRD explicitly chooses one (default remains improved bare CSS); Windows; release packaging (G04).

## Task map

| Task | Role |
| --- | --- |
| [UI-01](../10-25-ui-01-tokens-shell/prd.md) | First implementable child: tokens + shell |
| [UI-02](../10-25-ui-02-four-screens/prd.md) | Planning until UI-01 merges |
| [UI-03](../10-25-ui-03-metrics-forms/prd.md) | Planning until UI-01 merges |
