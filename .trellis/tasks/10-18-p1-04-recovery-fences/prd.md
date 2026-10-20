# P1-04: Establish lifecycle journals and stale-result fences

## Goal

Establish crash-safe operation boundaries before native secrets and collection jobs can mutate state.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-03](../10-18-p1-03-sqlite/prd.md)

Acceptance mapping: A18, A20, A21, A23. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Implement nonsecret lifecycle-operation tracking, external opaque secret-reference journal and deletion ledger with durable atomic-write handling.
- Add per-connection mutation fences, generations, optimistic configuration revisions, and recovery-before-scheduling orchestration.
- Implement a fake-store recovery harness for candidate activation, compensation, pending cleanup, delete fencing, and restore admission checks.

## Acceptance criteria

- [x] Injected interruptions before/after journal write, candidate activation, and cleanup converge idempotently or remain visibly pending. Corrupt journals block unsafe recovery.
- [x] An obsolete generation cannot commit while disable/delete/rotation runs; the final check occurs in the write transaction under the mutation fence.
- [x] A deletion ledger prevents dispatch and rejects restoration of a manifest containing deleted opaque IDs. Failure does not acknowledge deletion.
- [x] Journal files contain no credentials, provider identity, or usage data. Failure to establish durability stops mutation before secret writes.

## Out of scope

No real provider identity verification, complete credential UI, full managed backup creation/purge/restore, or release deletion certification.

## Planning status

Implemented and validated on branch `jondong/p1-04-recovery-fences`. All four acceptance criteria are evidenced by synthetic fixtures in `crates/infrastructure/src/lifecycle/tests.rs` and `crates/infrastructure/src/durable.rs`; see [validation.md](validation.md). Prototype contributions only: release acceptance for A18, A20, A21, and A23 remains Pending.
