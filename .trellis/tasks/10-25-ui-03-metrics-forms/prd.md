# UI-03: MetricValue, provenance, and Connections form readability

## Goal

Make MetricValue, provenance/status presentation, and Connections forms easier to scan after UI-01 tokens exist.

## Baseline and prerequisites

Parent [UI polish](../10-25-ui-polish/prd.md). Depends on [UI-01](../10-25-ui-01-tokens-shell/prd.md).

## Scope

- Visual clarity for `MetricValue` and provenance/freshness/coverage cues.
- Connections form field grouping, labels, and error/status readability.
- No change to validation rules, secret handling, or IPC shapes.

## Acceptance criteria (planning — not ready to implement)

- [ ] Metric and provenance presentation uses shared tokens and remains honest about synthetic/partial data.
- [ ] Connections form remains keyboard-accessible with visible focus and bilingual labels.
- [ ] No credential plaintext paths; session-only/native store behaviour untouched.
- [ ] Desktop frontend checks green.

## Out of scope / constraints

Same hard constraints as UI-01. Planning only until UI-01 is merged. Do not touch real Keychain entries or production databases.
