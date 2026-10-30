# P2-04 validation record

## Scope and prerequisites

Backend scope: `crates/infrastructure` (new `refresh::router::ProviderRouter` plus one module declaration), `crates/providers` (`Clone` on both reqwest transports, `SimulatedRouter::is_single`), `apps/desktop/src-tauri` (`RefreshState` type alias and `build_inner` composition). No core, contracts, frontend, or docs change; `Cargo.lock` untouched (the `quotavista-providers` edge in `crates/infrastructure/Cargo.toml` moved from `[dev-dependencies]` to `[dependencies]` — same workspace member, lockfile-stable).

Prerequisites confirmed: P2-02/P2-03 are archived on main and their live adapters (`OpenRouterAdapter`, `KimiMembershipAdapter`) compile with their offline fixture suites green.

What the change does: refresh no longer hands the coordinator a bare `SimulatedRouter`. `ProviderRouter` selects the adapter per connection from the stored `Connection.provider` id: any id starting with `synthetic` stays on the simulated demo adapters (Kimi side chosen by the `kimi` substring, mirroring `SimulatedRouter::route`, so existing fixtures such as `synthetic-provider-…` keep working); `kimi`/`openrouter` build a live adapter per call from the reqwest transport plus the credential resolved via `credential_ref` through the composed `SecretStore`. Credential resolution failures (no store, no reference, read error, invalid secret) all yield a `None` token so the adapter itself fails `PermissionRequired` before any request; a missing transport (init failure) fails `Offline`; an unknown provider id fails `InvalidResponse`. None of these paths touches the network. Secret reads are offloaded with `tokio::task::spawn_blocking` (same pattern as `reconcile_connection_credentials`). The coordinator phase machine is untouched, so the Phase 1 invariants (concurrency two, coalesce, persisted cooldowns, generation fences) hold by construction and by the unchanged 16-test coordinator suite.

Demo mode stays incapable of issuing provider requests by construction: demo connections resolve to the simulated side before any credential or transport is consulted, and the router tests assert the recording fake transports observed zero calls for every synthetic and every failure path.

Activation: `export TRELLIS_CONTEXT_ID=quotavista-2c8ac49d-5e28-4fa3-bef2-49b4e9f0ed5f` then `python3 .trellis/scripts/task.py start 10-29-p2-04-refresh-wiring` (planning → in_progress, `task.json` only).

Environment: macOS 27.0.1 arm64, Rust 1.99.1.

## Validation status

All required local validation passed on this worktree, re-run after the final code edit.

Commands run, in order, with real results:

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean, no diff (exit 0).
- `cargo test --locked --workspace` — every suite `ok` with 0 failures: contracts integration 16, core lib 31, desktop lib 83, infrastructure lib 77 (+1 ignored native), providers lib 72 (+2 ignored live), contracts doctests 13 — 292 passed / 0 failed / 3 ignored total. Baseline before this task was 280 passed (P2-03 state); the +12 are the 11 new router tests and 1 new desktop test.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — 0 `error` lines in the raw log (checked with `grep -cE '^error'` → `0`), run last after the final edit. Two findings were fixed during the run: dead `SYNTHETIC_KIMI`/`SYNTHETIC_OPENROUTER` constants after the prefix-routing switch (folded into one `SYNTHETIC_PREFIX`), and a useless `as_ref` in a test helper. The only warnings in the log are the pre-existing ts-rs `failed to parse serde attribute` notes from `quotavista-contracts`, unrelated to this change.
- `./scripts/check-core-boundaries.sh` — `quotavista-core calendar-only dependencies: boundary OK`.
- `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` — `TypeScript contract is current` (no DTO changed).
- `./scripts/native-launch-probe.sh` — `PROBE: PASS`. It exercises the real binary (real `ProviderRouter` + real reqwest transports, no credentials configured): cold launch seeds and refreshes both demo connections through the simulated side and commits the expected fixture (`data_revision` 8, 2 connections active, 6 bucket facts, 2 published partitions, every field-by-field check "as expected"), warm restart preserves the fixture and marks the demo connections `needs_credentials` exactly as before, and the `QUOTAVISTA_TRAY=off` run passes identically.
- Secret scan on the working diff (`git diff | grep -icnE 'sk-or-v1-[a-z0-9]{20,}|bearer [a-z0-9]{12,}|password'` → `0`): no credential material in the diff; test secrets are literal `test-token`/`kimi-token`/`openrouter-token` strings staged into in-memory `SessionSecrets` stores only.

## Acceptance-criteria evidence

| PRD criterion | Evidence (real test names) |
| --- | --- |
| Live adapter jobs participate in refresh without contacting providers from demo mode | `crates/infrastructure/src/refresh/router/tests.rs`: `demo_connection_uses_simulated_adapter_and_never_calls_transports` (both recording transports observe 0 calls while the demo job commits Synthetic snapshots), `live_kimi_connection_resolves_credential_and_commits_observation` (1 Kimi call, 0 OpenRouter calls, snapshot committed with `Provenance::Observation`, connection health `Success`), `live_openrouter_connection_uses_only_the_openrouter_transport` (1 OpenRouter call, 0 Kimi calls, Observation metrics + `key-budget` Unsupported row committed). Desktop wiring: `apps/desktop/src-tauri/src/lib.rs` test `live_connection_without_credential_fails_typed_without_network` drives the real composition (real transports) end-to-end through `refresh_connections` and gets a typed `Failed`/`PermissionRequired`/`ProvideCredentials` operation without network, twice deterministically. |
| Failure retention/isolation tests with fakes | `dataset_failure_is_isolated_and_prior_snapshots_are_retained` (HTTP 401 → only `prepaid-credits` fails with `PermissionRequired` health; its two previously committed snapshots are byte-identical afterwards; the sibling connection still commits), `whole_call_failure_retains_prior_snapshots` (transport Offline fails the whole call; prior snapshots retained), `missing_credential_fails_permission_required_without_network` (0 transport calls), `rate_limit_persists_cooldown_and_blocks_readmission_without_network` (429 with `Retry-After: 120` → `Outcome::Cooldown` at now+120 s, `cooldowns.v1` durable entry present, readmission returns `CooldownBlocked`, still exactly 1 transport call), `missing_transport_marks_live_connection_offline_without_network`, `degraded_composition_without_secrets_keeps_demo_connections_working` (no secret store at all: demo refresh still commits, live connection fails typed). |
| No regression in the synthetic path | The 16 pre-existing coordinator tests in `refresh/tests.rs` are untouched and green; all 82 pre-existing desktop lib tests are green against the new composition (including the `fixture_with_router` scenario tests that seed `synthetic-provider-…` ids); the 12 simulated-adapter tests in providers are untouched and green. `single_adapter_override_answers_live_connections_without_network` pins the `SimulatedRouter::single` override used by desktop test fixtures. |
| Demo mode cannot issue provider requests | Construction-level: the demo path never consults transports or credentials (`demo_connection_uses_simulated_adapter_and_never_calls_transports` asserts 0 recorded calls on both fake transports; `degraded_composition_without_secrets_keeps_demo_connections_working` repeats it with no secret store). The live adapters' own P2-02/P2-03 guarantees (no request without a token) hold underneath as a second layer. |

## Release-criteria status (honest mapping)

- A06 (offline/timeout/auth/rate-limit failures retain last values; other connections still update): **strengthened at the composition seam** — the router tests above show retention and per-connection/per-dataset isolation through the real coordinator with live adapters in the loop; the underlying retention semantics are the Phase 1 coordinator's and unchanged.
- A07 (freshness/allowed refresh after wake): **no change** — scheduling, tick, and freshness logic untouched; persisted cooldown behavior is exercised end-to-end through the live path by `rate_limit_persists_cooldown_and_blocks_readmission_without_network`.
- A01/A04: unchanged from P2-02/P2-03 — live reconciliation is still blocked on locally supplied provider credentials; this task wires the path but makes no live call.

## Live reconciliation blocker

No live provider credentials were used; every test runs against recording fake transports with scripted responses. The Kimi membership base URL remains the unvalidated placeholder from P2-03 (`https://www.kimi.com`), so a live connection with a real session token may fail `InvalidResponse`/`Offline` until P2-03's reconciliation lands. The env-gated live smoke tests from P2-02/P2-03 stay `#[ignore]`d and were not run (no keys configured).
