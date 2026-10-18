# Repository guidance

## Documentation language

All project documentation must be written in English. This includes README files, requirements, architecture, plans, installation and development instructions, contribution guidance, and diagram labels.

Keep this convention when adding or updating documentation. Conversations with users may follow the user's preferred language.

## Project context

Read README.md, docs/requirements.md, docs/architecture.md, and docs/development-plan.md before implementation work.

The project contains design documents, an MIT License, and a minimal P1-01 scaffold (Cargo workspace plus a synthetic-data-only Tauri window). Do not describe planned capabilities as implemented.

Token-availability research is deferred to provider integration. Keep real credentials and identifying account data out of the repository.

## Trellis workflow

Tasks and project-specific coding guidance are managed with Trellis 0.6.17. Read `docs/task-management.md`, `.trellis/workflow.md`, and the relevant `.trellis/spec/backend/index.md` or `.trellis/spec/frontend/index.md` before task work.

Use the existing Phase 1 task tree rather than duplicating tasks. Read the selected task's `prd.md`, `design.md`, and `implement.md`; check dependency completion before activation. Task hierarchy does not enforce dependencies. Product requirements and architecture remain authoritative over task summaries.

Development uses a multi-agent model. A coordinator manages work with Orca: each child task runs in its own Orca-managed git worktree with a dedicated pi agent, and the coordinator may pick a different provider and model per task. Independent tasks whose dependencies are complete may run in parallel. Each worker owns exactly one task, works inline inside its worktree, and does not dispatch further agents. Merges to `main` require the maintainer's approval. Honor explicit user authorization already present in the conversation without asking for the same setup, task-creation, or implementation permission again. Follow the approved scope and complete task planning before product implementation.

All task artifacts, specs, journals, and commit messages must be English. Keep runtime pointers and developer identity local; never record credentials or identifying account data in Trellis artifacts.
