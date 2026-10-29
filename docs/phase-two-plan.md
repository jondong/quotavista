# Phase 2 provider validation task plan

Date: 2026-10-29

Status: Planning boarded. Implementation starts with P2-02 (OpenRouter live adapter skeleton) once this plan and the Trellis tree exist on a branch; live reconciliation waits on locally supplied credentials. Phase 1 parent `10-18-phase-one-prototype` remains open and incomplete — Ubuntu/Linux Phase 1 validation is waived for now by product owner direction and must not be invented as OBSERVED, archived as complete, or rewritten to PASS.

Baseline: [Requirements 0.9](requirements.md), [Architecture 0.4](architecture.md), and [development plan](development-plan.md). [Trellis workflow instructions](task-management.md) describe setup and task operations. Acceptance mapping: [traceability](traceability.md).

## Outcome and scope

Validate real OpenRouter prepaid and Kimi shared-membership integrations against official sources using credentials the user supplies only on their machine. Produce capability reports and sanitized fixtures, wire live adapters through existing provider ports and refresh coordination, and prove same-scope credential rotation without leaking secrets into the repository, prompts, logs, or CI.

This phase closes the G01/G02 evidence path for A01–A04 and contributes live evidence toward A06–A07. It does not deliver full historical backfill (Phase 3), complete alert episodes (Phase 4), or release packaging / Ubuntu release-matrix work (Phase 5). Demo/synthetic adapters remain available and must stay clearly marked `Provenance::Synthetic`.

## Phase boundaries relative to Phase 1

| Carried from Phase 1 | Phase 2 must prove |
| --- | --- |
| Provider ports, typed errors, dataset isolation | Live adapters return the same shapes with `Provenance` that is not synthetic |
| Simulated capability fixtures and refresh coordinator | Capability reports for real OpenRouter and Kimi scopes; refresh admits live jobs |
| Native / session-only secret stores | Same-scope credential rotation and explicit identity replacement with store↔DB ordering failures |
| Synthetic identity validation in connection flows | Real account/key scope checks without inventing balances when permission is missing |

Ubuntu 26.04 GNOME rows from Phase 1 stay UNVERIFIED. Do not fabricate OBSERVED rows. Phase 5 retains Linux release validation.

## Task inventory

The [parent milestone](../.trellis/tasks/10-29-phase-two-provider-validation/prd.md) owns cross-task acceptance. Each child has a PRD with observable acceptance, a technical design, an ordered execution plan, and implementation/review context manifests. Checkboxes track evidence in each task's PRD; they do not imply full-release acceptance until reconciliation evidence exists.

| Task | Deliverable | Prerequisites | Initial state |
| --- | --- | --- | --- |
| [P2-00](../.trellis/tasks/10-29-p2-00-plan/prd.md) | Write this plan and board the Trellis parent/children | None | In progress on this branch |
| [P2-01](../.trellis/tasks/10-29-p2-01-capability-fixtures/prd.md) | Capability reports + sanitized fixtures (no secrets) | P2-00 | Planning |
| [P2-02](../.trellis/tasks/10-29-p2-02-openrouter-adapter/prd.md) | OpenRouter live adapter (Credits / Activity / Analytics ports) | P2-00; soft on P2-01 (minimal fixture schema may fold into P2-02) | Planning; recommended first code slice |
| [P2-03](../.trellis/tasks/10-29-p2-03-kimi-adapter/prd.md) | Kimi shared-membership adapter | P2-00; soft on P2-01 | Planning |
| [P2-04](../.trellis/tasks/10-29-p2-04-refresh-wiring/prd.md) | Wire live adapters into refresh / connection selection | P2-02 or P2-03 (at least one live adapter) | Planning |
| [P2-05](../.trellis/tasks/10-29-p2-05-credential-rotation/prd.md) | Same-scope credential rotation and identity replacement | P2-04 | Planning |

The table records the initial plan, not live task status. Use `task.py list` and each `task.json` for current status. Completed tasks move to `.trellis/tasks/archive/YYYY-MM/`.

## Suggested execution order

1. **P2-00** lands the plan and Trellis tree. Archive after the plan and tree exist on a reviewed branch/PR.
2. **P2-02** starts as soon as P2-00 artifacts are on disk, even if the P2-00 PR is not merged. Prefer a skeleton adapter + fixture stubs without a live Management Key; live Credits/Activity reconciliation waits until Jonathan supplies a Management Key locally (never into git, Issues, or prompts).
3. **P2-01** can run beside P2-02 or absorb OpenRouter fixture stubs from P2-02. Do not block P2-02 on a full capability research write-up; fold a minimal sanitized schema into P2-02 when needed.
4. **P2-03** follows the same pattern for Kimi shared membership (authorization against the official membership page; no cookies or identifying payloads in fixtures).
5. **P2-04** wires whichever live adapters exist into the existing refresh coordinator (one job per connection, concurrency two, typed errors, last-successful-value preservation).
6. **P2-05** proves same-scope rotation and explicit identity replacement, including failures between native secret-store and database steps, without inference calls.

Tasks in different lanes may run in parallel once prerequisites are complete. Prefer not starting P2-04 until at least one of P2-02/P2-03 has a reviewable adapter surface. Pre-resolve shared-file conflicts (especially `crates/providers`, `crates/infrastructure/src/refresh*`, connection IPC) before launching parallel implementers.

## Acceptance notes (Phase 2 coverage)

| ID | Phase 2 expectation |
| --- | --- |
| A01 | Live adapters retrieve core data from a real Kimi shared membership pool and an OpenRouter prepaid account; demo/synthetic data does not qualify for exit. Skeleton work may land earlier with fixtures marked incomplete for A01. |
| A02 | Kimi membership pool is shown without substituting an API wallet or independent Code limit; Web/app/Code shared consumption counted once. |
| A03 | Kimi totals match the official membership page for the same period within explained reporting delay or display precision. |
| A04 | OpenRouter account Credits stay distinct from key budget; missing Management Key permission does not invent an account balance. |
| A06 | Offline, timeout, auth, and rate-limit paths retain last committed values and isolate connections; live adapters feed the same typed errors as Phase 1. |
| A07 | Last-successful time and staleness remain honest after live refresh; wake/manual-only behavior continues to honor coordinator rules. |

A05 remains primarily Phase 1 transport/UI evidence; live adapters must not collapse real zero, unknown, unsupported, stale, or failed into each other. Full A06–A07 release closure still shares Phase 4 lifecycle work; Phase 2 contributes live refresh evidence only.

## Capability reports and sanitized fixtures (P2-01)

P2-01 published two capability documents and a sanitized fixture corpus. They are
documentation plus parse-time and hygiene evidence only:

- [`docs/validation/provider-capability-openrouter.md`](validation/provider-capability-openrouter.md)
  separates the four OpenRouter surfaces — account Credits, current key budget,
  Activity, and Analytics — records which required metrics **no documented surface
  can satisfy**, and lists the open questions a live reconciliation must answer.
- [`docs/validation/provider-capability-kimi.md`](validation/provider-capability-kimi.md)
  records the shared membership pool, its subscription-cycle monthly refresh, the
  1000-record usage-detail history bound, the officially documented usage-detail
  delay, and the explicit refusal to let the Kimi Open Platform API wallet or an
  independent Kimi Code limit substitute for membership quota. It also records that
  the membership pool has **no documented public JSON API**, and that the one
  machine-readable candidate found belongs to another tool's local runtime and is
  labelled experimental — so the retrieval path stays an open integration question.
- [`docs/validation/fixtures/`](validation/fixtures/) holds invented-identifier JSON
  shapes with sibling `*.meta.json` provenance, and
  `scripts/check-validation-fixtures.mjs` fails on malformed fixture content, on any
  of nine secret-like patterns, or on a capability report that cites a fixture path
  that does not exist.

Neither document claims a measured capability. Every capability verdict in both is
**Documented** or **Not retrievable**, never **Verified**: no live request was made,
no member's subscription page was compared, and no OpenRouter console figure was
reconciled. A01, A02, A03, and A04 therefore remain **Pending**, and the absence of
live keys stays an open credential blocker rather than a documentation pass.

## Credential and safety rules

- Users supply credentials only on their machine (native secret store or explicitly chosen session memory). Never commit keys, cookies, Management Keys, raw provider payloads with identifying account data, or authorization headers.
- Prompts under `~/.quotavista-orchestration/` must not embed secrets. Reference env/secret-store names only.
- OpenRouter Management Keys have administrative capabilities beyond inference keys: query only the required Credits / Activity / Analytics endpoints; do not create, modify, or delete keys or other account resources.
- Fixtures in the repository must be synthetic or explicitly sanitized. Prefer redacted JSON shapes and invented identifiers over live captures.
- Docs and Trellis artifacts stay English. No production credentials in CI.

## Workflow (local-first)

Standing QuotaVista rules apply:

1. Implementer works in an Orca worktree with a dedicated pi agent (create worktree without `--agent`, then `orca terminal create` with `PI_SUBAGENT_CHILD=1 PI_LENS_SUBAGENT_FULL=1 pi --model <provider/model> --thinking high`; wait `tui-idle` and confirm the status-bar model before dispatch).
2. Independent reviewer (different provider than the implementer; prefer `openai/gpt-6.1-sol` or `xai/grok-4.7`) reviews the local branch first.
3. Only after APPROVE: push, open PR, run CI.
4. Standing merge permission: when independent review APPROVEs the current head, CI is green, and the PR has no conflicts, merge via `cursor-github` with the sha guard, then tell Jonathan.
5. Implementer model rotation: `kimi-coding/k3`, `zai-coding-cn/glm-5.3`, `qwen-token-plan-cn/qwen3.8-max`, `minimax-cn/MiniMax-M3.1-Flash`.

Do not merge without the sha-guard standing rules. Do not archive the Phase 1 parent as complete.

## Exit gate

Reconcile real OpenRouter balance (and Activity/Analytics coverage where available) and Kimi shared quota against official sources; publish capability reports and sanitized fixtures; record unresolved integration issues explicitly. Parent milestone stays open until every child criterion passes with evidence that identifies commit, command/procedure, environment, result, and limitation. Live-key absence is an open credential blocker for A01/A03/A04 reconciliation — not a documentation PASS.

## Starting point

Board P2-00 through P2-05 under parent `10-29-phase-two-provider-validation`. Start P2-02 OpenRouter implementer on a separate worktree once this plan text and Trellis children exist on disk. Live Management Key and Kimi auth material remain human-supplied blockers for reconciliation slices.
