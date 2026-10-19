# Phase 1 prototype task plan

Date: 2026-10-18

Status: Task decomposition complete; application implementation has not started

Baseline: [Requirements 0.9](requirements.md), [Architecture 0.4](architecture.md), and [development plan](development-plan.md). [Trellis workflow instructions](task-management.md) describe setup and task operations.

## Outcome and scope

Create a runnable local desktop prototype that demonstrates Overview, Statistics, Connections, and Settings in English and Simplified Chinese using explicitly synthetic data. Establish module boundaries, exact accounting/time contracts, persistence/recovery, restricted IPC, native credentials, and usable desktop fallbacks before real provider work.

This is a foundation for the first release. It does not establish working Kimi or OpenRouter integrations, real usage totals, full provider history, complete alert logic, retention/managed restore, or release packaging. Token availability and provider authentication research remain in Phase 2.

## Task inventory

The [parent milestone](../.trellis/tasks/10-18-phase-one-prototype/prd.md) owns cross-task acceptance. Each child has a PRD with observable acceptance, a technical design, an ordered execution plan, and implementation/review context manifests. Checkboxes track evidence in each task's PRD; they do not imply full-release acceptance.

| Task | Deliverable | Prerequisites | Initial state |
| --- | --- | --- | --- |
| [P1-01](../.trellis/tasks/archive/2026-10/10-18-p1-01-workspace/prd.md) | Scaffold the desktop workspace and reproducible checks | None | Completed and archived; PR #1 merged |
| [P1-02](../.trellis/tasks/10-18-p1-02-contracts/prd.md) | Define domain primitives and checked IPC contracts | P1-01 | Implemented with synthetic fixtures; awaiting PR review |
| [P1-03](../.trellis/tasks/10-18-p1-03-sqlite/prd.md) | Implement SQLite persistence and candidate publication | P1-02 | Planning |
| [P1-04](../.trellis/tasks/10-18-p1-04-recovery-fences/prd.md) | Establish lifecycle journals and stale-result fences | P1-03 | Planning |
| [P1-05](../.trellis/tasks/10-18-p1-05-secret-stores/prd.md) | Implement native and session-only secret storage | P1-04 | Planning |
| [P1-06](../.trellis/tasks/10-18-p1-06-simulation/prd.md) | Add deterministic simulated providers and refresh coordination | P1-04 | Planning |
| [P1-07](../.trellis/tasks/10-18-p1-07-ipc/prd.md) | Compose the backend behind restricted Tauri IPC | P1-05, P1-06 | Planning |
| [P1-08](../.trellis/tasks/10-18-p1-08-bilingual-shell/prd.md) | Build the bilingual desktop navigation and display foundations | P1-02 | Planning |
| [P1-09](../.trellis/tasks/10-18-p1-09-overview-statistics/prd.md) | Implement simulated overview and period statistics views | P1-07, P1-08 | Planning |
| [P1-10](../.trellis/tasks/10-18-p1-10-connections-settings/prd.md) | Implement prototype connection and settings flows | P1-07, P1-08 | Planning |
| [P1-11](../.trellis/tasks/10-18-p1-11-desktop-lifecycle/prd.md) | Implement desktop lifecycle and native integration fallbacks | P1-07, P1-08 | Planning |
| [P1-12](../.trellis/tasks/10-18-p1-12-integration-validation/prd.md) | Validate prototype recovery, bilingual flows, and local query budgets | P1-09, P1-10, P1-11 | Planning |
| [P1-13](../.trellis/tasks/10-18-p1-13-native-validation/prd.md) | Validate native macOS and Ubuntu prototype behavior | P1-12 | Planning |

The table records the initial plan, not live task status. Use `task.py list` and each `task.json` for current status. Completed tasks move to `.trellis/tasks/archive/YYYY-MM/`; resolve their stable task directory name there after archiving. These initial planning links may need updating as tasks move.

## Suggested execution order

1. P1-01 establishes the workspace and baseline CI; P1-02 establishes contracts and arithmetic/time primitives.
2. P1-03 and P1-04 establish persistence and recovery. P1-08 can follow P1-02 without waiting for native adapters.
3. P1-05 and P1-06 establish native secrets and deterministic collection; P1-07 composes restricted IPC.
4. P1-09, P1-10, and P1-11 integrate the screens and desktop behavior after IPC and the bilingual shell.
5. P1-12 reviews integrated behavior and measures local query performance. P1-13 validates the actual target desktops and assesses Phase 1 exit.

These are dependency lanes. Tasks in different lanes may run in parallel once their prerequisites are complete. Each child task gets its own Orca-managed worktree and a dedicated pi agent, which owns that one task and works inline; the coordinator selects the provider and model per task and integrates results into `main` after review.

## Phase boundaries

| Foundation in Phase 1 | Completion deferred |
| --- | --- |
| Synthetic adapter capabilities and deterministic history fixtures | Actual authorization, account reconciliation, metric availability, and pagination research in Phase 2 |
| Checked time/numeric contracts and fixture-backed display queries | Full provider synchronization, retention, and statistics correctness in Phase 3 |
| Candidate history/publication and stale-generation safety | Provider-specific authoritative revision handling in Phase 3 |
| Durable journals, deletion fence, and restore-admission harness | Complete managed backup purge/restore and production lifecycle walkthroughs in Phase 4 |
| Local connection flows with synthetic identity validation | Real same-account verification and provider-specific replacement in Phase 2; complete flows in Phase 4 |
| Notification capability smoke check and threshold preferences | Persistent threshold/expiration episodes and deduplication in Phase 4 |
| Initial CI, local query benchmark, and native viability evidence | Release matrix, minimum macOS version, signing/installers and full acceptance in Phase 5 |

## Exit review and evidence

The milestone remains open until every child and parent criterion passes. Evidence must identify the commit, command/procedure, environment, result, and limitation. A missed benchmark target is an open performance blocker; an unavailable Ubuntu desktop is an unverified native gate. Neither can be marked complete through documentation alone.

Use 90 days and 100,000 synthetic facts for the query benchmark, with five warmups and at least 30 measured queries; record query mix and P95 against the one-second target and test the five-second cancellation bound.

Record macOS OS/architecture and Ubuntu 26.04/GNOME/x86_64 with the actual Wayland/X11 session type. Check cold/warm launch, local recovery, native stores, tray/no-tray, notification availability, close/reopen/quit/wake, and both UI locales. CI builds are supplementary evidence.

G01/G02 remain open until real provider validation. G04 retains release work even when prototype native checks pass. Requirements A01-A25 remain governed by the acceptance mapping; prototype tests contribute evidence without closing unrelated release obligations.

## Starting point

The next implementation target is P1-01. Its prerequisites are empty. All other child tasks wait for the predecessors recorded in `meta.depends_on` and their PRD/execution plan. Trellis does not enforce this dependency graph; the task owner must verify completed active or archived records before activation. The parent is a milestone and should not be activated as a substitute for child work.
