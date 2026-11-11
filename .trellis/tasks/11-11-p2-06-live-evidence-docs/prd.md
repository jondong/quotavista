# P2-06: Live-smoke evidence and auth guidance corrections

## Goal

Record the user-reported OpenRouter live-smoke result with honest limitations, correct misleading Management Key and Kimi authentication guidance in English docs and Trellis, and assess the desktop OpenRouter validation path with non-secret automated tests only.

## Acceptance

- [x] OpenRouter user-reported live smoke from 2026-11-10 21:52 Asia/Shanghai is recorded (command class, environment category, result category, limitations). It is labelled user-reported, not independently reproduced, and not numeric console or desktop UI reconciliation.
- [x] Docs and Trellis no longer claim that an OpenRouter Management Key is read-only or that a vendor-scoped read-only Management Key permission exists. Guidance recommends a short-expiry validation key and revocation after use, plus client-side query-only discipline.
- [x] Kimi blockers are updated from "waiting on a user session token" to "unresolved supported Kimi integration". Adapter base URL and `/api/membership/shared-pool` remain unvalidated placeholders. Moonshot and Kimi Code API keys are not substitutes. The write-up does not claim universal impossibility or drop Kimi, and does not ask anyone to extract browser cookies or another tool's credentials.
- [x] A01, A02, A03, and A04 are each given a distinct status against `docs/phase-two-plan.md` definitions. None is marked PASS on the basis of this smoke output. The Phase 2 parent stays open. The Phase 1 parent stays incomplete and is not archived or marked PASS.
- [x] Desktop OpenRouter connection/refresh code and existing automated tests are inspected; non-secret automated checks are run. Remaining exact human desktop numeric-reconciliation steps are written down. No live UI verification is claimed.
- [x] No secrets, identifying account data, or live ignored tests are introduced. No extra providers or features are implemented. No `prod/` changes.

## Mapping

Contributes evidence notes toward A01/A04 (OpenRouter smoke only) and corrects G01/G02 guidance. Does not close Phase 2 exit acceptance.
