# P1-02: Define domain primitives and checked IPC contracts

## Goal

Make accounting, time, lifecycle, and transport semantics explicit before connecting UI and storage.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-01](../archive/2026-10/10-18-p1-01-workspace/prd.md)

Acceptance mapping: A05, A13, A14, A17, A18, A22, A25. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Implement unit-safe token, decimal-money, ratio, period, provenance, coverage, and independent connection/dataset states in core.
- Define all reviewed command/envelope/error/operation and metric/statistics DTOs, including revisions, request IDs, effective intervals, exclusion reasons, and locale settings. Generate or mechanically check TypeScript types.
- Add a fake clock and initial calendar-range conversion helpers: inclusive dates to half-open intervals, Monday weeks, as-of clipping, 366-day validation, and source-boundary compatibility.

## Acceptance criteria

- [x] Token facts support u64 and checked u128 totals; DTO round trips preserve counts above JavaScript safe integers. Decimal precision limits reject excess precision without rounding.
- [x] Tests distinguish zero, null, unsupported, partial coverage, stale values, and dataset failure. Cached/reasoning subcounts are not implicitly added twice.
- [x] DST transitions, Monday boundaries, reversed/future/oversized ranges, unknown zones, and current-day clipping have meaningful tests.
- [x] A mechanically checked Rust/TypeScript contract includes all command names, locale preference, and safe typed errors without serializing domain types directly.

## Out of scope

No live endpoint schema, universal token availability assumption, complete statistics engine, or provider authentication research.

## Planning status

Implemented and validated with synthetic-only fixtures; criterion-level evidence and commands are recorded in [validation.md](validation.md). Awaiting maintainer PR review. This task is not archived, and its prototype evidence does not close full-release acceptance.
