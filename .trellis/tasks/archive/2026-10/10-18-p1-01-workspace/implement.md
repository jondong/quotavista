# P1-01 execution plan

## Entry checks

- [ ] Read the PRD, design, relevant specs, and reviewed baseline.
- [ ] Confirm predecessor tasks are completed, including archived records: None; first implementation task.
- [ ] Review the plan against any newer decisions and existing user authorization before activation.

## Work sequence

- [ ] Inspect available toolchains and current official Tauri prerequisites; record version choices.
- [ ] Scaffold a minimal workspace and local window; add lockfiles, scripts, and CI.
- [ ] Run the documented check commands and a macOS launch smoke check; update contributor instructions and specs with actual paths.

## Validation and review

- [ ] Clean lockfile-based installation and production frontend build.
- [ ] cargo fmt --all -- --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace.
- [ ] Run the new frontend type-check/lint scripts and desktop development command; record exact commands after creation.
- [ ] Record evidence for each PRD criterion, review the complete diff, and update only affected specs/traceability.
- [ ] Run `git diff --check`; commit with an English message before archiving.

## Recovery and completion

If a check fails, keep the task incomplete and preserve the last committed valid state. Use synthetic temporary stores and restore test fixtures instead of touching personal data. Record native/provider gaps explicitly. Archive only when all task acceptance criteria are satisfied; the phase parent remains open until its cross-task exit gate passes.
