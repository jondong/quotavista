# P2-02: OpenRouter live adapter

## Goal

Implement an OpenRouter adapter behind existing provider ports for account Credits and available Activity/Analytics queries, starting with a skeleton and sanitized fixtures; live reconciliation when a Management Key is available locally.

## Baseline and prerequisites

Baseline as in [phase-two-plan](../../../docs/phase-two-plan.md). Dependencies: [P2-00](../10-29-p2-00-plan/prd.md). Soft on P2-01 — fold minimal fixture schema here if needed.

Acceptance mapping: A01, A04 primary; A06/A07 typed-error and freshness contributions. A01/A04 PASS requires live reconciliation evidence.

## Scope and deliverables

- `crates/providers` OpenRouter module implementing the provider port for current Credits (total_credits / total_usage → remaining) and history/analytics where supported.
- Explicit permission-gated behavior when a regular key lacks Management scope — do not invent account balance.
- Sanitized fixtures and unit tests offline; optional local integration test gated on env/secret-store presence (never in CI secrets).
- Query only required endpoints; no key create/modify/delete.

## Acceptance criteria

- [x] Adapter compiles and unit tests pass against fixtures without network.
- [x] Account Credits metrics use non-synthetic provenance when live; key budget remains a distinct capability.
- [x] Missing Management permission yields `PermissionRequired` (or equivalent typed error) without fabricating Credits.
- [x] Offline/timeout/429 map to existing `ProviderError` variants; rate-limit cooldown propagated.
- [x] No secrets in repo, logs assertions, or test output.
- [x] Live reconciliation notes recorded when Jonathan supplies a Management Key (blocker until then).

## Out of scope

Kimi adapter (P2-03); full refresh wiring (P2-04); credential rotation UX (P2-05); inference calls.
