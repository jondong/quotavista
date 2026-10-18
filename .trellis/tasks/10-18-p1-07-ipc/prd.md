# P1-07: Compose the backend behind restricted Tauri IPC

## Goal

Connect the prototype frontend to local services through explicit validated commands.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-05](../10-18-p1-05-secret-stores/prd.md), [P1-06](../10-18-p1-06-simulation/prd.md)

Acceptance mapping: A05, A09, A18, A21, A22, A23. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Compose storage, synthetic providers, coordinator, settings, operations, and secret-store ports in the desktop entry point.
- Expose reviewed commands with validated DTO envelopes, safe errors, optimistic revisions, operation polling, and revision-only events. Unsupported later-phase actions must return explicit safe states.
- Configure trusted local-window capabilities and command permissions; add a narrow generated-type IPC client.

## Acceptance criteria

- [ ] Malformed/unknown fields, providers, IDs, revisions, dates, and oversized credentials fail before side effects. Only connect_provider and replace_credentials accept secret input; no response echoes it.
- [ ] The renderer cannot invoke a generic URL/SQL/shell/filesystem proxy, access native secrets directly, or gain privileges through remote content.
- [ ] Revision events contain opaque IDs and revisions only. Reopening reads local state; skipped revisions trigger reload. Obsolete display responses are discarded.
- [ ] Storage/secret/coordinator errors remain typed, useful cached data survives, and failed commits never report synchronization success.

## Out of scope

No arbitrary diagnostic endpoints, remote login window, cloud API, or complete real-provider operations.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
