# P2-02 validation record

## Scope and prerequisites

Backend scope only, `crates/providers`. P2-02 adds the OpenRouter live-adapter skeleton behind the existing `ProviderPort` boundary: a query-only Credits implementation (`/api/v1/credits`, Management Key), an injectable transport + auth-handle boundary, sanitized wire fixtures, and offline unit tests. No core, contracts, infrastructure, desktop, or frontend change is included beyond the `quotavista-providers` crate root re-export and `Cargo.toml`/`Cargo.lock` dependency additions (`serde`, `serde_json` with `raw_value`, `reqwest 0.13` with `rustls` + `http2` + `system-proxy`, default features off). `crates/core/src/provider.rs` is untouched this task.

Prerequisite confirmed: P2-00 artifacts exist at `.trellis/tasks/10-29-p2-00-plan` (planning docs, advisory dependency). P2-01 (capability fixtures) has no fixtures on disk yet, so per the soft-dependency fold-in rule this task carries the minimal sanitized fixture set for the endpoints it touches, plus one shape-only Activity stub for later tasks.

Every fixture is sanitized: invented values, redacted identifiers, synthetic key-shaped strings only. No live account data was read, no Management Key was available or used, no network call happened in default tests, and no credential value was written into any file, prompt, or artifact.

Activation: `export TRELLIS_CONTEXT_ID=quotavista-2c8ac49d-5e28-4fa3-bef2-49b4e9f0ed5f` then `python3 .trellis/scripts/task.py start 10-29-p2-02-openrouter-adapter` (planning → in_progress, `task.json` only).

Environment: macOS 27.0.1 arm64, Rust 1.99.1.

## Validation status

All required local validation passed on this worktree, re-run after the final code edit.

Commands run, in order, with real results:

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean, no diff (exit 0).
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — 0 `error` lines in the raw log (checked with `grep -cE '^error'` → `0`). One failure was fixed during the runs: an unused `use std::cell::Cell;` left in the test module after the fake transport switched to `Mutex`/`AtomicUsize`. The only warnings in the log are the pre-existing ts-rs `failed to parse serde attribute` notes from `quotavista-contracts`, unrelated to this change.
- `cargo test --locked --workspace` — every suite `ok` with 0 failures: contracts integration 16, core lib 31, desktop lib 82, infrastructure 66 (+1 ignored native), providers 43 (+1 ignored live), contracts doctests 13 — 251 passed / 0 failed / 2 ignored total. Within `quotavista-providers` the pre-existing 13 simulated-adapter tests stay green (demo adapters untouched) and 30 offline OpenRouter tests pass; 1 test is `#[ignore]`-gated (live smoke).
- `cargo test --locked -p quotavista-providers quotavista_test_openrouter_live_credits_management_key -- --ignored --nocapture` — passes with the documented skip note when no key is supplied: `skipping live test: QUOTAVISTA_OPENROUTER_MANAGEMENT_KEY is not set (Management Key must be supplied locally; never committed)`.
- `./scripts/check-core-boundaries.sh` — `quotavista-core calendar-only dependencies: boundary OK`.
- `./scripts/test-check-core-boundaries.sh` — `all boundary-gate regression checks passed`.
- `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` — `TypeScript contract is current` (no DTO changed).
- Secret scan on the staged diff (`git diff --cached | grep -inE 'sk-or-v1-[a-z0-9]{20,}|api[_-]?key\s*[:=]|bearer [a-z0-9]|password|secret\s*[:=]\s*"[^"]'`) — the only remaining hits are doc-comment prose about the `BearerToken` concept and the synthetic test token `sk-or-v1-synthetic-test-token` / `sk-or-v1-synthetic-not-a-real-key`, which never authenticate anywhere (spec: synthetic credentials allowed in fixtures/tests). No real or live credential appears in the diff.

## Acceptance-criteria evidence

| PRD criterion | Evidence (real test names, all in `crates/providers/src/openrouter/`) |
| --- | --- |
| Adapter compiles and unit tests pass against fixtures without network | `cargo test --locked --workspace` green (providers lib 43 passed / 0 failed / 1 ignored). Every default test injects a fake transport or parses `include_str!` fixtures: `credits_success_maps_money_metrics`, `parses_string_amounts_from_fixture`, `parses_bare_number_amounts_exactly`, `transport_offline_fails_whole_call`, `transport_timeout_fails_whole_call`. The real transport is only exercised by the `#[ignore]` live test. |
| Account Credits metrics use non-synthetic provenance when live; key budget remains a distinct capability | Credits metrics are `Provenance::Observation` + `QueryKind::Unvalidated` (never `Synthetic`, never `ValidatedCurrent` — validation is claimed only by capability validation): asserted in `parses_string_amounts_from_fixture`. Distinctness: `key_budget_is_reported_unsupported_not_conflated` asserts the `key-budget` dataset gets its own explicit `Unsupported` row and is never merged into `prepaid-credits`. Dataset id deliberately reuses the synthetic demo's `prepaid-credits` spelling so account balance stays one concept across demo and live adapters. |
| Missing Management permission yields `PermissionRequired` without fabricating Credits | Two layers: (a) no credential at all → whole-call failure before any network traffic, asserted by `missing_token_fails_whole_call_without_network` (also asserts the fake transport recorded 0 calls); (b) a key that authenticates but lacks Management scope (HTTP 401/403) → only the `prepaid-credits` dataset fails while `key-budget` keeps its row: `forbidden_maps_to_dataset_permission_failure`, `unauthorized_maps_to_dataset_permission_failure`. No path invents a balance: absence of parseable data is `InvalidResponse` (`missing_amount_field_is_invalid`, `malformed_body_is_invalid`), never a zero. |
| Offline/timeout/429 map to existing `ProviderError` variants; rate-limit cooldown propagated | `transport_offline_fails_whole_call` → `ProviderError::Offline`; `transport_timeout_fails_whole_call` → `Timeout`; `server_error_fails_whole_call_invalid_response` (received-but-unusable 503 and 404 → `InvalidResponse`; `Offline` is reserved for transport-level network failure). 429 → `RateLimited { retry_after }` with the cooldown parsed from `Retry-After`: delta-seconds (`rate_limit_carries_cooldown_from_retry_after_seconds`), HTTP-date relative to the request instant (`rate_limit_parses_http_date_against_request_instant`), absent-header default 60 s (`rate_limit_without_header_uses_default_cooldown`), clamping/fallback semantics (`retry_after_parsing_clamps_and_falls_back`), and hostile-value regression (`retry_after_hostile_values_never_panic` — a digit run past chrono's `TimeDelta::seconds` panic threshold and a year-9999 HTTP-date both clamp to 24 h; a 32-digit run falls back; `Retry-After: 0` stays zero; no input can panic). The coordinator can persist this through the existing `.cooldown()`/`Cooldowns` path unchanged. |
| No secrets in repo, logs assertions, or test output | Secret scan above; `BearerToken` redaction asserted by tests: `bearer_token_debug_and_display_redact_the_secret` (Debug and Display show `[redacted]`, never the token) and `adapter_debug_redacts_auth` (adapter Debug shows `<redacted>` for the auth source). Invalid-token validation asserted by `bearer_token_rejects_invalid_secrets` (empty, CRLF injection, >64 KiB — the shared `secrets::validate_secret` rules). No log statement in the new module prints tokens, headers, URLs with query, or bodies; the live test prints only error categories and dataset names. |
| Live reconciliation notes recorded (blocker until Management Key is supplied locally) | See “Live reconciliation blocker” below. The `#[ignore]` live smoke test `quotavista_test_openrouter_live_credits_management_key` is runnable exactly when Jonathan supplies a query-only Management Key locally: `QUOTAVISTA_OPENROUTER_MANAGEMENT_KEY=<key> cargo test -p quotavista-providers -- --ignored`. It never runs in CI or default `cargo test`. |

Additional invariants covered beyond the PRD boxes:

- Exact decimal accounting: amounts parsed from raw JSON text (string or bare number), never through `f64`: `parses_bare_number_amounts_exactly`, `reject_signed_exponent_and_escaped_amounts` (signs, exponents, objects, null all rejected), `precision_violation_is_invalid` (>18 fractional digits rejected), `subtraction_handles_scale_mismatch_exactly` (18-digit alignment exactness). Subtraction is manual coefficient alignment because core `Decimal` has no `checked_sub` and is nonnegative-only.
- Overdraft honesty: when usage exceeds credits the remainder is negative, which the nonnegative money domain cannot carry — the `remaining` metric is emitted `Missing`/`IncompatibleUnit` with no value (a unit-system limit, not a missing upstream capability), and the two directly observed totals stay `Available`: `negative_remaining_is_reported_missing_not_clamped`. Never clamped to zero.
- Endpoint-failure isolation: systemic failures fail the whole call; endpoint-specific ones fail only their dataset while other datasets keep committed rows: `oversized_body_fails_credits_dataset_only`, `malformed_success_body_fails_credits_dataset_only`, `non_utf8_body_fails_credits_dataset_only`.
- History safety: `history_is_not_served_yet` asserts `history_page` returns `Ok(None)` with and without a cursor, so committed history stays untouched instead of serving unvalidated bucket shapes.
- Transport hygiene: `base_url_must_be_https` (http:// and file:// rejected), `transport_debug_shows_base_url_only` (Debug carries no client internals), 256 KiB response cap enforced before parsing.

## Release-criteria status (honest mapping)

- A01 (real adapters retrieve core data): **not PASS** — that requires live reconciliation evidence. This skeleton's fixtures-only state is the P2-02 deliverable exactly as the phase-two plan describes; the live path is implemented and env-gated but unexercised.
- A04 (account balance vs key budget separation, no invented balance): code-complete and tested offline (`key_budget_is_reported_unsupported_not_conflated`, permission-failure paths above); live confirmation pending the Management Key.
- A06 (offline/timeout/429/permission failures preserve data + typed errors): satisfied at the adapter boundary — typed `ProviderError` variants throughout, last-committed data preserved by construction (failures return errors or `Missing`/`Unsupported` rows, never overwrites with zeros).
- A07 (freshness): partial — `observed_at` records the local observation instant, while `source_as_of` stays `None` until reconciliation confirms a server-side timestamp field in the Credits envelope (no server timestamp is claimed that the endpoint did not vouch for); flagged TODO for reconciliation.

## Live reconciliation blocker

**No Management Key was available during this task, so no live OpenRouter call has been made.** Wire shapes in `crates/providers/src/openrouter/fixtures/*.json` are sanitized inventions and must be confirmed against the real endpoint before any acceptance criterion that needs live evidence (A01/A04 PASS) is claimed. Specifically pending:

1. Confirm field spellings/casing (`data.total_credits`, `data.total_usage`) and whether amounts arrive as strings or numbers (both are accepted; strictness stays).
2. Confirm the account currency is always USD (`CREDITS_CURRENCY` constant) or map a returned currency field.
3. Confirm whether the envelope carries a server timestamp; if so, map it to `source_as_of` (currently `None` — never a fabricated instant) during reconciliation.
4. Confirm the 401 vs 403 distinction for a key without Management scope, and the real 429 `Retry-After` shape.
5. Run the live smoke test locally with the key and record results here; promote `QueryKind::Unvalidated` → validated classification only after the capability-validation process, not in the adapter.

The adapter never creates, modifies, or deletes keys: it issues exactly one GET to `/api/v1/credits` per `current()` call and nothing else. Management Key delivery at runtime is a higher-layer concern (native secret store / session memory through `AuthTokenSource`); the adapter never reads environment variables in library code — the documented env var is read only inside the `#[ignore]` test.

## Review boundaries

Local-first: committed on `jondong/p2-02-openrouter-adapter` in this worktree; no push, no PR. Independent review (different provider/model than the implementer) is the coordinator's next step. Activity/Analytics beyond the shape-only stub, key-budget querying, refresh wiring (P2-04), and rotation (P2-05) remain open for later tasks.

## Review round 1 fixes (independent review of 9b18a3f/98371f0)

The independent review returned REQUEST_CHANGES; every blocking and should-fix item was applied in this worktree and all gates re-run green afterwards.

- **B1 (blocking) — hostile `Retry-After` panic**: `parse_retry_after` now does all arithmetic in raw clamped `i64` seconds (checked `timestamp()` subtraction for HTTP-dates; digit runs parsed to `i64` then clamped before any `Duration` construction). No panicking duration constructor can see an out-of-range value. Regression test `retry_after_hostile_values_never_panic`: `9223372036854776` (past chrono's `TimeDelta::seconds` panic threshold) and `Fri, 31 Dec 9999 23:59:59 GMT` both clamp to 24 h; a 32-digit run falls back to 60 s; `Retry-After: 0` is preserved as zero (immediate retry).
- **S1 — `source_as_of` fabrication**: all emitted metrics (credits totals, remaining, overdraft row, key-budget row) now carry `source_as_of: None` instead of the request instant. `observed_at` keeps the local observation instant. No server freshness is claimed until reconciliation confirms an envelope timestamp; asserted in `parses_string_amounts_from_fixture`.
- **S2 — overdraft reason**: the overdraft `remaining` row is now `Missing`/`Reason::IncompatibleUnit` (unit-system limit) instead of `Unsupported` (capability absence); asserted in `negative_remaining_is_reported_missing_not_clamped`.
- **S3 — received-but-unusable statuses**: 400/404/422/5xx now map to `ProviderError::InvalidResponse` (whole call) instead of `Offline`; `Offline` remains transport-level only (connect/DNS/TLS). Asserted in `server_error_fails_whole_call_invalid_response` (503 and 404 cases).
- **S4 — streaming body cap**: the 256 KiB cap is now enforced while downloading (`response.chunk()` loop with per-chunk accumulate-and-check), not only post-hoc after `bytes()`; a missing or understated `Content-Length` can no longer buffer an unbounded body first. Oversized bodies still surface as `TransportError::Oversized` → dataset-level `InvalidResponse` (`oversized_body_fails_credits_dataset_only`).
- **S5 — redirect hardening**: the reqwest client is built with `redirect::Policy::none()`, so a redirect of the credentialed GET is never followed and the Management Key cannot leak to another origin or path; a 3xx arrives as a status and is classified `InvalidResponse` by the S3 arm.
- **S6 — proxy/HTTP2 parity**: `reqwest` features extended to `rustls` + `http2` + `system-proxy` (default features still off), so machines behind a system proxy are not silently `Offline` on the live path.
- Nits: duplicated doc comments on `ReqwestTransport::new`/`with_base_url` and the test `FakeTransport` removed; journal session 6 summary filled in and its commit table now lists `98371f0`; `.trellis/spec/backend/index.md` no longer describes `crates/providers` as synthetic-only; the fixture `credits_number_amounts.json` now uses non-f64-representable bare numbers (120.055 / 37.257 → remaining 82.798) so `parses_bare_number_amounts_exactly` proves the raw-text path rather than passing by f64 luck; this file's suite counts corrected (contracts integration 16, core lib 31, desktop lib 82, infrastructure 66 +1 ignored, providers 43 +1 ignored, contracts doctests 13).

All validation commands in “Validation status” were re-run after these edits with the results recorded above (251 passed / 0 failed / 2 ignored workspace-wide).
