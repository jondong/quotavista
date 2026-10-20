# P1-05: Implement native and session-only secret storage

## Goal

Validate credential isolation through explicit native stores and an opt-in memory-only path.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-04](../archive/2026-10/10-18-p1-04-recovery-fences/prd.md)

Acceptance mapping: A10, A18, A21. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Implement the SecretStore port with pinned keyring-core and explicit macOS Keychain/Linux Secret Service adapters; isolate blocking calls.
- Connect candidate-write/activate/cleanup operations to lifecycle journals using a synthetic identity validator.
- Add explicit session-only backend storage and distinguish locked, unavailable, and needs-credentials states.

## Acceptance criteria

- [x] Synthetic create/read/rotate/delete works on each available native store; rotation failure keeps the previous active credential and exposes failed compensation.
- [x] Locked and unavailable store errors remain distinct. No file or plaintext persistence fallback occurs.
- [x] An explicit session-only choice works until exit; restart retains nonsecret data, has no retrievable session secret, and enters Needs credentials.
- [x] Representative success/error/serialization paths do not leak the synthetic secret into SQLite, journals, logs, events, or DTOs. Native Linux evidence remains pending when unavailable.

## Out of scope

No browser-cookie import, real credentials, provider authorization, or automatic downgrade to session-only storage.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
