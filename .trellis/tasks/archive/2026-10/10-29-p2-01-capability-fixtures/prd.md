# P2-01: Capability reports and sanitized fixtures

## Goal

Document what OpenRouter and Kimi can and cannot expose for QuotaVista's required metrics, and land synthetic or sanitized fixtures without secrets.

## Baseline and prerequisites

Baseline as in [phase-two-plan](../../../docs/phase-two-plan.md). Dependencies: [P2-00](../10-29-p2-00-plan/prd.md). Soft coupling with P2-02/P2-03 (fixtures may originate there).

Acceptance mapping contributions: A01–A04 research evidence; fixtures support A05/A06 testing. Live reconciliation still requires P2-02/P2-03 with real credentials.

## Scope and deliverables

- Capability reports covering permissions, account vs key scope, token/history availability, bucket boundaries, unsupported required metrics.
- Sanitized fixture files (shapes only; invented IDs; no cookies, auth headers, or identifying account data).

## Acceptance criteria

- [ ] OpenRouter capability report distinguishes Management Key Credits, key budget, Activity (last 30 UTC days), and Analytics; marks unsupported metrics explicitly.
- [ ] Kimi capability report covers shared membership pool, refresh timing, history limits, and what cannot substitute (API wallet, independent Code limits).
- [ ] Fixture corpus builds and is referenced by adapter tests; contains no secrets.

## Out of scope

Live network calls in CI; full backfill design (Phase 3).
