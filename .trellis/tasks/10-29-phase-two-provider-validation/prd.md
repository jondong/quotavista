# Phase 2: Initial provider validation

## Goal

Validate QuotaVista's first real provider integrations — OpenRouter prepaid Credits and Kimi shared membership quota — against official sources, using credentials supplied only on the user's machine.

## Authoritative baseline

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), [Phase 2 task plan](../../../docs/phase-two-plan.md), and [acceptance traceability](../../../docs/traceability.md).

## Scope

Capability reports and sanitized fixtures; OpenRouter live adapter; Kimi shared-membership adapter; refresh wiring for live adapters; same-scope credential rotation. Docs English. No secrets in repository or prompts.

## Phase exit acceptance

- [ ] Every child task is completed with review and evidence.
- [ ] OpenRouter account Credits (and available Activity/Analytics coverage) reconcile against the console for a locally supplied Management Key; key budget stays distinct from account balance (A01, A04).
- [ ] Kimi shared membership quota reconciles against the official membership page for the same period within explained delay/precision (A01–A03).
- [ ] Capability reports list supported, unsupported, and permission-gated metrics explicitly; fixtures contain no secrets or identifying account data.
- [ ] Live refresh failures preserve last-successful values and isolate connections (A06); freshness/staleness remain honest (A07 contributions).
- [ ] Same-scope credential rotation and explicit identity replacement are evidenced, including store↔DB failure ordering (P2-05).
- [ ] Traceability distinguishes live reconciliation from fixture/skeleton work; Phase 1 parent remains open incomplete; no invented Ubuntu OBSERVED rows.

## Out of scope and deferred gates

No full historical backfill/retention (Phase 3), complete alert episodes (Phase 4), or release packaging / Ubuntu release-matrix closure (Phase 5). Inference traffic is out of scope. Phase 1 Ubuntu waiver does not convert UNVERIFIED Linux rows to PASS.

## Task map and planning status

See [Phase 2 task plan](../../../docs/phase-two-plan.md). This parent is a milestone, not an implementation target. Do not activate the parent as a substitute for child work.
