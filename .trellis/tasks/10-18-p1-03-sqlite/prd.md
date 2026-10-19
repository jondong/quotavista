# P1-03: Implement SQLite persistence and candidate publication

## Goal

Persist synthetic monitoring state safely across restarts without confusing candidates with verified history.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-02](../archive/2026-10/10-18-p1-02-contracts/prd.md)

Acceptance mapping: A09, A16, A19, A22, A23. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Add versioned migrations for the reviewed connection/scope/product, snapshot/fact, coverage/checkpoint, alert/settings, lifecycle/sync/staging/published-partition, and managed-backup metadata boundaries. Keep later-phase behavior explicitly incomplete.
- Implement backend repositories, WAL, serialized writes, per-connection foreign keys and busy timeout, exact-number storage, and basic settings/snapshot reads and writes.
- Exercise candidate page/cursor transactions and atomic complete-partition replacement through a synthetic repository harness.

## Acceptance criteria

- [x] Fresh migration, reopen, and forced transaction failure preserve committed values and exact numeric strings; failed writes do not advance successful sync time.
- [x] A candidate page and its cursor commit together. An interrupted candidate does not overwrite or add to its previous published partition.
- [x] A complete authoritative replacement removes facts absent from the replacement; repeated publication is idempotent. Event identity remains stable when a timestamp is corrected.
- [x] Tests confirm settings for every pooled connection, bounded busy errors, migration failure behavior, and the selected linked SQLite version.

## Out of scope

No provider pagination/backfill, full 90-day retention, managed backup/restore implementation, or complete usage analytics.

## Planning status

Implemented and synthetic-tested against the reviewed baseline. The evidence for all four task criteria is recorded in [validation.md](validation.md); full release criteria remain Pending. Final branch validation, PR review, and green PR CI are required before handoff. The task remains active and must not be archived by this worker.
