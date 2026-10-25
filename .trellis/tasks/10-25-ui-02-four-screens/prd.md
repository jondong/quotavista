# UI-02: Four-screen layout polish

## Goal

Improve layout density and visual hierarchy on Overview, Statistics, Connections, and Settings after UI-01 tokens exist.

## Baseline and prerequisites

Parent [UI polish](../10-25-ui-polish/prd.md). Depends on [UI-01](../10-25-ui-01-tokens-shell/prd.md).

## Scope

- Consume UI-01 tokens for cards, tables, sections, and empty/loading/error frames on the four screens.
- Improve spacing rhythm and hierarchy without changing screen behaviour or IPC payloads.

## Acceptance criteria (planning — not ready to implement)

- [ ] Each of the four screens uses shared tokens for surfaces, borders, and spacing.
- [ ] Loading/empty/error foundations remain bilingual and accessible.
- [ ] No IPC or contract changes; desktop frontend checks green.

## Out of scope / constraints

Same hard constraints as UI-01 (no UI library by default; synthetic only; no Keychain/prod DB; no Phase 2 provider work). Planning only until UI-01 is merged.
