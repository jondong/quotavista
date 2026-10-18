# QuotaVista Development Plan

Date: 2026-10-18

Status: Follow the architecture baseline; phase completion requires validation

This plan maps the [requirements](requirements.md) and [architecture](architecture.md) to acceptance gates. The repository currently contains documentation and the MIT License, not a runnable application. Token-availability research remains deferred to provider integration.

All project documentation, diagram labels, and contributor guidance must be in English.

## Phase 1 Desktop and local core prototype

Validate that the proposed stack runs in target environments and establish replaceable provider and persistence boundaries.

- Initialize Tauri 2, React, TypeScript, Vite, and the Rust workspace; pin dependencies.
- Establish overview, statistics, connections, settings, and display contracts using clearly labeled simulated data.
- Implement SQLite migrations, reads, and writes; verify restart recovery and transaction rollback.
- Validate macOS Keychain and Linux Secret Service, including explicitly authorized session-only behavior when a store is unavailable.
- Target Ubuntu 26.04 with GNOME on x86_64 for Linux. Validate the window, tray, notifications, and close behavior; record the actual session type and establish the initial support matrix.
- Add meaningful Rust domain tests and TypeScript checks. Do not present simulated accounts as connected real accounts.

Exit gate: The prototype runs on target macOS and Linux environments, credentials are not persisted in plaintext, and Linux remains usable without a tray. If Linux is unavailable for validation, identify that gap instead of claiming cross-platform completion.

Requirements covered: Foundations for A05, A09, A10, A11, and A12.

## Phase 2 Initial provider validation

- Validate OpenRouter permissions, account scope, balance, and distinction from key budget.
- Validate authorization and retrieval of Kimi's shared membership quota against the official subscription page.
- Investigate token availability, current-day coverage, history limits, bucket boundaries, quota details, and provider revisions.
- Produce capability reports and synthetic or sanitized response fixtures without cookies, headers containing secrets, or identifying account data.
- Review unsupported required metrics explicitly instead of substituting another metric.
- Implement per-connection refresh, task merging, authentication handling, and last-successful-value preservation.

Exit gate: Reconcile real balance and shared quota; establish statistics capabilities and authorization boundaries; record unresolved integration issues. Users supply credentials locally, never through public Issues or version control.

Requirements covered: A01, A02, A03, A04, A06, and A07.

## Phase 3 Historical synchronization and statistics

- Implement official event or bucket backfill, pagination, coverage, and checkpoints.
- Verify that repeated synchronization, revisions, late data, and partial failures do not duplicate consumption or lose confirmed facts.
- Implement Today, Yesterday, This week, Last week, This month, Last 7 days, Last 30 days, and custom ranges.
- Provide daily and weekly totals, trends, and plan and account breakdowns.
- Validate token inclusion rules, currencies, quota changes, and time boundaries; mark unavailable metrics.
- Implement 90-day retention with matching coverage cleanup while retaining latest current values.
- Test midnight, quota resets, daylight-saving changes, UTC-only day buckets, and collection gaps.

Exit gate: Match official data for available intervals, avoid filling gaps with zero or treating incomplete tokens as complete, and keep repeated refresh idempotent.

Requirements covered: A13 through A17.

## Phase 4 Alerts and complete user flows

- Implement quota, balance, expiration, and connection alerts, configurable thresholds, and notification disabling.
- Verify period deduplication, recovery, restart behavior, and suppression of new alerts from stale cache.
- Complete connection creation, update, disable, removal, and deletion-failure recovery.
- Validate network recovery, wake, and application exit.
- Check keyboard access and state communication using both text and color.

Exit gate: Complete flows are usable, polling does not resend alerts, and outdated tasks cannot restore a deleted connection.

Requirements covered: A06 through A10, refresh, and connection management.

## Phase 5 Cross-platform release

- Validate Linux on the confirmed Ubuntu 26.04, GNOME, x86_64 target. Establish macOS versions and architectures and record Linux session types.
- Build native packages and validate installation, migration during upgrades, launch, and removal.
- Finish English installation and development documentation, contributor guidance, known limitations, and third-party notices.
- Cover domain tests, type checks, formatting, and target-platform builds in CI. Use UI tests for essential interactions.
- Review A01 through A17 and distinguish real-account validation from fixture-based failure testing.

Exit gate: Required support-matrix checks pass, release notes describe data and platform limitations, and public artifacts contain no credentials or identifying account data.

## Validation principles

Use working artifacts, data reconciliation, and relevant tests as evidence. Documentation checks are not application tests, and successful builds do not prove platform usability.

Use synthetic or explicitly sanitized provider fixtures. Cover statistical, credential, and persistence branches that could misreport spending or leak secrets; do not create tests that merely mirror simple display code.

Do not promise unverified interfaces, fixed delivery dates, or unmeasured performance. When findings affect requirements, update the corresponding requirements, architecture, and development plan.
