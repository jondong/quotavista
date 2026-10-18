# Phase 1: Desktop and local core prototype

## Goal

Validate QuotaVista's proposed desktop stack and local boundaries before real provider integration. Deliver a runnable, bilingual prototype using unmistakably synthetic Kimi shared-membership and OpenRouter prepaid data.

## Authoritative baseline

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [acceptance traceability](../../../docs/traceability.md).

## Scope

Workspace and reproducible checks; checked domain/IPC types; SQLite persistence and publication boundaries; lifecycle journals and deletion fences; native/session-only credentials; deterministic providers; restricted IPC; four bilingual screens; desktop fallbacks; integrated and native evidence.

## Phase exit acceptance

- [ ] Every child task is completed with review and evidence, and the integrated prototype opens on the target macOS and Ubuntu 26.04/GNOME/x86_64 environments.
- [ ] Four screens render synthetic current and period data with correct units, exact backend values, explicit provenance, freshness, coverage, and safe failures in en and zh-CN.
- [ ] Restart preserves committed nonsecret state. Credentials remain in native stores or explicitly chosen backend session memory; no plaintext fallback exists.
- [ ] Candidate history, recovery journals, and stale-result fences pass focused interruption/race tests. Later-phase backup, retention, alert, and provider behaviors remain identified as incomplete.
- [ ] Linux is usable without a tray. Native checks record the actual session type, secret-store behavior, notifications, close/quit/wake, and launch observations.
- [ ] Automated checks pass and the documented 100,000-fact local-query benchmark has measured evidence or an explicitly unresolved performance blocker.
- [ ] Traceability distinguishes synthetic evidence from real-provider and release acceptance; no unavailable environment is reported as passed.

## Out of scope and deferred gates

No real provider calls, authentication/token-availability research, inference traffic, complete provider history/backfill, complete alert episodes, full retention/managed restore, release signing/installers, or Windows artifacts. G01/G02 remain for Phase 2; G04 retains release-matrix and packaging work. A synthetic prototype cannot satisfy A01-A04.

## Task map and planning status

See [Phase 1 task plan](../../../docs/phase-one-plan.md) for the dependency table and suggested order. This parent is a milestone, not an implementation target. All children start in planning. No new product decision blocks task decomposition; actual provider feasibility and native behavior remain evidence gates.
