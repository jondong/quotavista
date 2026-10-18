# P1-10: Implement prototype connection and settings flows

## Goal

Exercise local setup and preference flows safely using synthetic validation.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-07](../10-18-p1-07-ipc/prd.md), [P1-08](../10-18-p1-08-bilingual-shell/prd.md)

Acceptance mapping: A07, A09, A10, A18, A21, A25. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Build clearly simulated add/rename/disable/re-enable/rotate/remove operation views, including needs-credentials and deletion-pending recovery states.
- Support native-store choice and explicitly selected session-only mode with synthetic credentials; explain that no provider validation is occurring.
- Persist versioned refresh interval, reporting zone, UI locale, and threshold preferences through backend settings. Full alert evaluation remains later-phase work.

## Acceptance criteria

- [ ] Empty/oversized aliases, empty/oversized/header-invalid secrets, and stale revisions fail safely; secret fields clear on submission and never enter browser storage.
- [ ] Synthetic same-scope rotation preserves history; different/unverified identity cannot silently merge it. One active connection per provider is enforced even for racing candidates.
- [ ] Disable/remove fence late work; cleanup failures show Deletion pending and survive restart. Unimplemented managed-backup guarantees stay explicitly pending.
- [ ] Explicit locale and refresh-Off choices survive restart; session-only connections return Needs credentials without implicit collection. Locale changes preserve filters/form state.

## Out of scope

No real Kimi/OpenRouter login, payment management, multiple simultaneous accounts per provider, full managed restore, or alert-episode engine.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
