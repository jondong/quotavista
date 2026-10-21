# P1-11: Implement desktop lifecycle and native integration fallbacks

## Goal

Keep the prototype reachable and predictable with or without tray and notification services.

## Baseline and prerequisites

[Requirements 0.9](../../../docs/requirements.md), [Architecture 0.4](../../../docs/architecture.md), [development plan](../../../docs/development-plan.md), and [traceability](../../../docs/traceability.md).

Dependencies: [P1-07](../archive/2026-10/10-18-p1-07-ipc/prd.md), [P1-08](../archive/2026-10/10-18-p1-08-bilingual-shell/prd.md)

Acceptance mapping: A07, A11, A21, A25. These are prototype contributions, not closure of full-release acceptance.

## Scope and deliverables

- Implement macOS menu-bar/Linux tray entry points where available, an explicit Quit action, single-instance behavior, and main-window reopen.
- Add platform notification capability detection and an explicitly triggered localized synthetic notification; preserve an in-app explanation when unavailable.
- Integrate close/quit/wake behavior with job cancellation, committed persistence, refresh-Off semantics, and secret-store recovery ordering.

## Acceptance criteria

- [x] Without a usable tray, closing the main window exits; there is no inaccessible orphan collector. All essential flows remain reachable from the main window.
- [x] With a usable tray, users can reopen and explicitly quit. Quit cancels work and commits or rolls back active writes.
- [x] Synthetic notification uses the active locale and no identifying account details; unavailable delivery retains an in-app status. It does not establish real alert behavior.
- [x] Wake merges permitted refresh work; refresh-Off never initiates an implicit query. Record differences between available native targets.

## Out of scope

No background service, complete threshold/expiration alert engine, installer signing, or GNOME compatibility claim without a native run.

## Planning status

Ready for task review against the baseline. No unresolved product decision blocks this plan. Check technical choices against current official documentation during implementation. Implemented and validated; per-criterion evidence is recorded in `validation.md`, together with the interactive window, tray-menu and notification-banner observations that need a human at a desktop session and remain pending, and the Ubuntu 26.04 GNOME target handed to P1-13. Review round 1 requested changes; all three findings are addressed on this branch and recorded under `## Review round 1` in `validation.md`, and a real machine suspend/resume plus the interactive window, tray-menu and banner observations stay pending a human native run.
