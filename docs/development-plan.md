# QuotaVista Development Plan

Date: 2026-10-18

Status: Follow the architecture baseline; phase completion requires validation

This plan maps the [requirements](requirements.md) and [architecture](architecture.md) to acceptance gates. The repository currently contains documentation and the MIT License, not a runnable application. Token-availability research remains deferred to provider integration.

All project documentation, diagram labels, and contributor guidance must be in English.

## Design review baseline

Requirements 0.9 and Architecture 0.4 have completed an internal document review. The [requirements review](reviews/requirements-review.md), [architecture review](reviews/architecture-review.md), and [acceptance mapping](traceability.md) record findings, resolutions, and planned evidence for A01 through A25. This is prototype-design readiness, not user approval of every design choice or runtime validation.

The user confirmed both English and Simplified Chinese UI in P0. G03's product decision is closed; implementation must still pass A25. G01/G02 provider validation remains deferred to Phase 2, and G04 native release evidence remains open.

## Phase 1 Desktop and local core prototype

Validate that the proposed stack runs in target environments and establish replaceable provider and persistence boundaries.

The [Phase 1 task plan](phase-one-plan.md) breaks this work into a Trellis milestone and 13 dependency-aware child tasks. See [task management](task-management.md) for contributor setup and lifecycle commands. P1-01 through P1-12 are merged and archived; P1-13 is in progress and incomplete. No application capability beyond what those merged tasks record is claimed here.

- Initialize Tauri 2, React, TypeScript, Vite, and the Rust workspace; pin dependencies.
- Establish overview, statistics, connections, settings, and display contracts using clearly labeled simulated data.
- Bundle en and zh-CN catalogs, implement system-based first selection and persistent explicit choice, and keep locale independent of time zone and currency.
- Implement SQLite migrations, reads, and writes; verify restart recovery and transaction rollback.
- Establish operation journals, a deletion fence, candidate history staging, and transport error types before adding live provider behavior.
- Validate macOS Keychain and Linux Secret Service, including explicitly authorized session-only behavior when a store is unavailable.
- Target Ubuntu 26.04 with GNOME on x86_64 for Linux. Validate the window, tray, notifications, and close behavior; record the actual session type and establish the initial support matrix.
- Add meaningful Rust domain tests and TypeScript checks. Do not present simulated accounts as connected real accounts.

Exit gate: The prototype runs on target macOS and Linux environments, credentials are not persisted in plaintext, and Linux remains usable without a tray. If Linux is unavailable for validation, identify that gap instead of claiming cross-platform completion.

Exit-gate status (2026-10-21): **not met**. The [Phase 1 exit assessment](validation/phase-one-exit-assessment.md) records macOS non-interactive native checks as passing on the host that was available, the sixteen interactive desktop observations as pending a human operator, and every Linux row as unverified because no Ubuntu 26.04 GNOME x86_64 machine existed in this validation window. The gap is identified rather than covered by a cross-platform claim, exactly as this gate requires. G01, G02, and G04 remain open.

Requirements covered: Foundations for A05, A07, A09, A10, A11, A12, and A18 through A25, as mapped in the acceptance matrix.

## Phase 2 Initial provider validation

- Validate OpenRouter permissions, account scope, balance, and distinction from key budget.
- Validate authorization and retrieval of Kimi's shared membership quota against the official subscription page.
- Investigate token availability, current-day coverage, history limits, bucket boundaries, quota details, and provider revisions.
- Produce capability reports and synthetic or sanitized response fixtures without cookies, headers containing secrets, or identifying account data.
- Review unsupported required metrics explicitly instead of substituting another metric.
- Implement per-connection refresh, task merging, authentication handling, and last-successful-value preservation.
- Validate same-scope credential rotation and explicit identity replacement, including failures between native secret-store and database steps. Verify response and retry limits without making inference calls.

Exit gate: Reconcile real balance and shared quota; establish statistics capabilities and authorization boundaries; record unresolved integration issues. Users supply credentials locally, never through public Issues or version control.

Requirements covered: A01, A02, A03, A04, A06, and A07.

## Phase 3 Historical synchronization and statistics

- Implement official event or bucket backfill, pagination, coverage, and checkpoints.
- Verify that repeated synchronization, revisions, late data, and partial failures do not duplicate consumption or lose confirmed facts.
- Verify that removed records disappear only after a complete authoritative replacement publishes; partial runs keep their candidate cursor separate from published completeness.
- Implement Today, Yesterday, This week, Last week, This month, Last 7 days, Last 30 days, and custom ranges.
- Provide daily and weekly totals, trends, and plan and account breakdowns.
- Validate token inclusion rules, currencies, quota changes, and time boundaries; mark unavailable metrics.
- Implement 90-day retention with matching coverage cleanup while retaining latest current values.
- Test midnight, quota resets, daylight-saving changes, UTC-only day buckets, and collection gaps.
- Run the documented local statistics benchmark and validate cancellation, overflow, and decimal-precision failures.

Exit gate: Match official data for available intervals, avoid filling gaps with zero or treating incomplete tokens as complete, and keep repeated refresh idempotent.

Requirements covered: A13 through A17, A19, A20, A22, and A23.

## Phase 4 Alerts and complete user flows

- Implement quota, balance, expiration, and connection alerts, configurable thresholds, and notification disabling.
- Verify period deduplication, recovery, restart behavior, and suppression of new alerts from stale cache.
- Complete connection creation, update, disable, removal, and deletion-failure recovery.
- Test managed backup deletion and restore with pending journals; confirm no late write or restored backup recreates an acknowledged deletion.
- Validate network recovery, wake, and application exit.
- Check keyboard access and state communication using both text and color.
- Exercise complete flows and subsequent notifications in both UI languages, including switching during refresh without changing statistics or sending another alert.

Exit gate: Complete flows are usable, polling does not resend alerts, and outdated tasks cannot restore a deleted connection.

Requirements covered: A06 through A10, refresh, and connection management.

## Phase 5 Cross-platform release

- Validate Linux on the confirmed Ubuntu 26.04, GNOME, x86_64 target. Establish macOS versions and architectures and record Linux session types.
- Build native packages and validate installation, migration during upgrades, launch, and removal.
- Finish English installation and development documentation, contributor guidance, known limitations, and third-party notices.
- Cover domain tests, type checks, formatting, and target-platform builds in CI. Use UI tests for essential interactions.
- Review A01 through A25 using the acceptance mapping and distinguish real-account validation from fixture-based failure testing.

Exit gate: Required support-matrix checks pass, release notes describe data and platform limitations, and public artifacts contain no credentials or identifying account data.

## Validation principles

Use working artifacts, data reconciliation, and relevant tests as evidence. Documentation checks are not application tests, and successful builds do not prove platform usability.

Use synthetic or explicitly sanitized provider fixtures. Cover statistical, credential, and persistence branches that could misreport spending or leak secrets; do not create tests that merely mirror simple display code.

Do not promise unverified interfaces, fixed delivery dates, or unmeasured performance. When findings affect requirements, update the corresponding requirements, architecture, and development plan.
