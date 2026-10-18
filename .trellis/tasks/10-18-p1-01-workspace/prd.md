# P1-01: Scaffold the desktop workspace and reproducible checks

## Goal

Establish a minimal reproducible Tauri 2 desktop application and isolated Rust modules.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: None; first implementation task.

Acceptance mapping: A11, A12, A24. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Create the reviewed apps/desktop and crates/{core,providers,infrastructure} layout with dependency direction enforced by package boundaries.
- Pin a compatible Rust toolchain, Node/package-manager versions, dependencies, Cargo.lock, and one frontend lockfile. Document native prerequisites from current official documentation.
- Add a minimal local window, standard check/build commands, and a synthetic-data-only development default. Set up CI formatting, Clippy, Rust tests, TypeScript checks, frontend lint, and production builds; add checks from later tasks as they land.

## Acceptance criteria

- [ ] A fresh checkout can reproduce frontend and Rust dependency installation using the committed lockfiles and documented toolchain.
- [ ] The desktop opens a minimal window on the available macOS host without a QuotaVista cloud account or provider credential.
- [ ] Core does not import Tauri, SQLx, native credential stores, or concrete providers. CI runs the documented checks without credentials.
- [ ] Document the linked SQLite version and current upstream WAL-fix requirement when adding SQLx; do not claim a successful Linux native run from CI.

## Out of scope

No complete screens, provider integration, signing, installers, auto-updates, or minimum macOS support claim.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. All criteria remain pending; this task has not started.
