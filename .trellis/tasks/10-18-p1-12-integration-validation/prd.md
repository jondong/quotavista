# P1-12: Validate prototype recovery, bilingual flows, and local query budgets

## Goal

Review the integrated prototype against the design without equating synthetic evidence with release acceptance.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-09](../archive/2026-10/10-18-p1-09-overview-statistics/prd.md), [P1-10](../archive/2026-10/10-18-p1-10-connections-settings/prd.md), [P1-11](../archive/2026-10/10-18-p1-11-desktop-lifecycle/prd.md)

Acceptance mapping: A05, A09, A10, A13-A25 (prototype evidence only). These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Exercise the four-screen synthetic workflow in both languages, including pending reads, partial permissions/history, local failures, session-only restart, and no-tray behavior.
- Run focused crash/race/secret-isolation checks across storage, journals, secret-store fakes, and IPC. Update acceptance traceability with exact evidence and remaining gaps.
- Benchmark 90-day local queries over 100,000 synthetic facts with five warmups and at least 30 measured queries; record machine, query mix, P95, cancellation and failures.

## Acceptance criteria

- [ ] Required automated checks pass in CI and reproducibly locally; failures and limitations have owning follow-up tasks. No real credentials are required.
- [ ] A benchmark artifact records actual timings against the one-second P95 target and five-second cancellation bound; missed targets remain open and are not silently redefined.
- [ ] Review evidence distinguishes implemented behavior, synthetic tests, native checks, and provider/release work. A01-A04 and G01/G02 remain unverified.
- [ ] Both locale flows preserve exact statistics, reporting zones, selections, and safe recovery behavior. No critical prototype acceptance defect remains unowned.

## Out of scope

No real-account reconciliation, final release certification, provider capability research, or closure of G04 based on automated checks.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
