# Task management with Trellis

Date: 2026-10-18

## Installed setup

QuotaVista uses [Mindfold Trellis](https://github.com/mindfold-ai/Trellis), pinned to `@mindfoldhq/trellis@0.6.17`, with Codex support. It was initialized with:

```sh
npx --yes @mindfoldhq/trellis@0.6.17 init --codex -u jondong --yes --no-monorepo
```

This command is setup provenance, not a step to repeat on an existing checkout. The checked-in Python scripts run the everyday workflow without a global Trellis installation or Node dependency in the future application. Upstream requires Node.js 18 or later for its CLI and Python 3.9 or later for scripts. Setup was checked with Node.js 24.22.0, npm 11.19.1, and Python 3.15.0; these are observed tooling versions, not the application's future pinned toolchain.

Trellis was initialized before application workspaces exist. Its single-project backend/frontend spec layout is intentional. The future Cargo workspace and desktop frontend do not require separate Trellis packages yet; reevaluate package-scoped specs only when actual code ownership warrants it.

## Repository surfaces

- `.trellis/tasks/`: shared task records, PRDs, technical designs, execution plans, and context manifests.
- `.trellis/spec/`: focused project guidance grounded in reviewed requirements/architecture; add actual source examples as code lands.
- `.trellis/workflow.md`: installed workflow reference; repository instructions and existing user authorization take precedence over generic repeated-consent prompts.
- `.trellis/workspace/`: English developer journals containing nonsecret project progress only.
- `.agents/skills/`: generated Trellis skills available to compatible coding tools.
- `.codex/`: project-scoped Codex settings, agent definitions, and optional hook integration.

The root requirements and architecture remain authoritative. Tasks summarize a bounded deliverable; specs guide implementation and must not silently change product scope. See [Phase 1 task plan](phase-one-plan.md).

## First use in a checkout

Initialize your local developer identity once, using your own public contributor handle:

```sh
python3 .trellis/scripts/init_developer.py your-handle
python3 .trellis/scripts/get_context.py
python3 .trellis/scripts/task.py list
```

`.trellis/.developer`, runtime session pointers, and Python caches are ignored by Git. Shared tasks/specs and intentionally recorded nonsecret journals are versioned. Do not put keys, session cookies, raw provider payloads, identifying account data, or personal environment details into task artifacts.

## Working on an existing task

1. Select a child from `task.py list`. Read its `prd.md`, `design.md`, and `implement.md`, then the relevant backend/frontend specs and baseline documents.
2. Check every `meta.depends_on` predecessor is completed. Search active and archived task records by directory name; parent/child hierarchy and metadata do not enforce dependencies.
3. Validate task context, review the plan and existing user authorization, then activate the selected child.
4. Implement and verify its acceptance criteria; record evidence and update only affected guidance. Keep incomplete or unverified work open.
5. Commit the reviewed change with an English message, archive the completed child, and record a concise English session journal. Keep the parent milestone open until its own exit criteria pass.

For example, the first implementation task is:

```sh
python3 .trellis/scripts/task.py validate 10-18-p1-01-workspace
python3 .trellis/scripts/task.py start 10-18-p1-01-workspace
python3 .trellis/scripts/task.py current --source
```

`start` requires session identity. Codex hooks or a supported host environment may provide it. If the host does not, set a unique local `TRELLIS_CONTEXT_ID` for the current terminal/session before `start`; use the same value for subsequent commands in that session. For example:

```sh
export TRELLIS_CONTEXT_ID="quotavista-$(python3 -c 'import uuid; print(uuid.uuid4())')"
```

Do not commit that value. `create --no-start` is appropriate for backlog planning. `start` changes planning to in_progress; `finish` clears the session pointer without completing the task; `archive` records completed status and moves it to the monthly archive. Do not archive incomplete tasks merely to clear the active pointer.

```sh
python3 .trellis/scripts/task.py finish
python3 .trellis/scripts/task.py archive TASK-DIRECTORY --no-commit
python3 .trellis/scripts/add_session.py --title "English session title" --commit COMMIT-HASH --summary "Nonsecret progress and remaining gaps"
```

Set correct branch/base metadata when using a task branch. For a documentation-only task deliberately completed on main without a PR, `archive --skip-branch-validation --no-commit` is available; do not use it to bypass missing implementation review. `session_auto_commit: false` keeps archive/journal changes reviewable; commit them explicitly.

## Task state and dependencies

Use planning, in_progress, and review for active work; archive only after acceptance passes. Blockers belong in task notes/artifacts with the missing evidence or predecessor. Keep the baseline requirement IDs and phase boundaries intact.

Phase 1 custom metadata contains `phase`, `plan_id`, `depends_on`, and the baseline versions. Dependency values are stable task directory names and must resolve in active or archived records. This is an advisory graph, not a built-in scheduler. Before activation, manually check predecessor completion; when scope changes, update metadata and the matching prerequisite text together.

Each child currently has implementation and review JSONL manifests with real source-document references. Inline mode also reads task artifacts and specs directly. Add specific source/test files as they exist rather than referencing planned paths as though implemented.

## Codex integration

Development follows a multi-agent model: a coordinator uses Orca to give each child task its own git worktree and a dedicated pi agent, chooses the provider and model per task, and may run independent tasks in parallel once their dependencies are complete. `codex.dispatch_mode: inline` keeps each worker executing its own task inline rather than spawning nested Trellis sub-agents. No global Codex settings were changed during setup. Tasks and scripts work through explicit reads/commands even when hooks are inactive.

The generated hook integration is optional. The initializer reports that compatible Codex clients require user-level hooks enablement and may require a one-time `/hooks` approval. Installing the files does not prove hook execution in the current host. Follow the installed `.codex/config.toml` comments and current official client documentation if enabling hooks; this is not required to start task work manually.

## Updates and third-party notices

Keep `.trellis/.version` and update provenance in sync. Use the pinned CLI for inspection; do not automatically run `init --force` or replace customized specs/AGENTS.md with upstream defaults. Review workflow, skill, script, and hook diffs in a dedicated update task before adopting a new version.

The installed development tooling has its own upstream license; see [Trellis provenance](../.trellis/NOTICE.md). Do not include `.trellis`, `.agents`, or `.codex` in desktop bundles. QuotaVista's original application remains MIT licensed.
