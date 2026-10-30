# P2-03 validation record

## Scope and prerequisites

Backend scope only, `crates/providers`. P2-03 adds the Kimi shared-membership live-adapter skeleton behind the existing `ProviderPort` boundary: a query-only membership implementation (`GET /api/membership/shared-pool` placeholder path, session token via `SessionTokenSource`), an injectable transport + auth-handle boundary, sanitized wire fixtures, and offline unit tests. No core, contracts, infrastructure, desktop, or frontend change is included. Changes outside `crates/providers/src/kimi/` are exactly: `pub mod kimi;` in `crates/providers/src/lib.rs` (plus its module doc line) and a one-word visibility change in `crates/providers/src/openrouter/mod.rs` (`fn parse_retry_after` → `pub(crate) fn parse_retry_after`) so the Kimi adapter reuses the hardened `Retry-After` parser instead of duplicating it. No dependency was added to `Cargo.toml`/`Cargo.lock` (`chrono`, `serde`, `serde_json` with `raw_value`, `reqwest` were already present from P2-02). `crates/core` is untouched this task.

Prerequisites confirmed: P2-00 artifacts exist at `.trellis/tasks/10-29-p2-00-plan`; P2-02 (OpenRouter adapter) is merged and archived at `.trellis/tasks/archive/2026-10/10-29-p2-02-openrouter-adapter` and was used as the mirror pattern. P2-01 (capability fixtures) has no fixtures on disk yet, so per the soft-dependency fold-in rule this task carries its own minimal sanitized fixture set and records the limitation in the module-level docs; A01–A03 are explicitly not claimed.

Every fixture is sanitized: invented values only, no cookies, no raw headers, no identifiers, no account data. No live Kimi session was available or used, no network call happens in default tests, and no credential value was written into any file, prompt, or artifact.

Activation: `export TRELLIS_CONTEXT_ID=quotavista-e4ee633b-6cb4-4360-b64b-e640912efd76` then `python3 .trellis/scripts/task.py start 10-29-p2-03-kimi-adapter` (planning → in_progress, `task.json` only).

Environment: macOS 27.0.1 arm64, Rust 1.99.1.

## Validation status

All required local validation passed on this worktree, re-run after the final code edit.

Commands run, in order, with real results:

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean, no diff (exit 0).
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — 0 `error` lines in the raw log (checked with `grep -cE '^error'` → `0`). One compile error was fixed during development: a `u8 == char` comparison in `remaining_ratio`'s debug assertion (byte literal fix). The only warnings in the log are the pre-existing ts-rs `failed to parse serde attribute` notes from `quotavista-contracts`, unrelated to this change.
- `cargo test --locked --workspace` — every suite `ok` with 0 failures: contracts integration 16, core lib 31, desktop lib 82, infrastructure 66 (+1 ignored native), providers 72 (+2 ignored live), contracts doctests 13 — 280 passed / 0 failed / 3 ignored total. Within `quotavista-providers` the pre-existing 43 tests (13 simulated + 30 OpenRouter) stay green — synthetic demo adapters untouched and still `Provenance::Synthetic` — and 29 new offline Kimi tests pass; 1 Kimi test is `#[ignore]`-gated (live smoke). Two development-time failures were wrong test expectations, not logic bugs, and were corrected before this record (`99.995` consumed → ratio `0.00005` is the exact value; the fixture epoch constant had been written for 2025 instead of 2026).
- `cargo test --locked -p quotavista-providers quotavista_test_kimi_live_membership_session_token -- --ignored --nocapture` — passes with the documented skip note when no token is supplied: `skipping live test: QUOTAVISTA_KIMI_SESSION_TOKEN is not set (session token must be supplied locally; never committed)`.
- `./scripts/check-core-boundaries.sh` — exit 0 (boundary OK).
- `./scripts/test-check-core-boundaries.sh` — `PASS: 13 passed` at the time of the final run.
- `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` — exit 0, contract is current (no DTO changed).
- Secret scan on the staged diff (`git diff --cached | grep -inE 'sk-or|api[_-]?key|password|cookie|authorization:|bearer [a-z0-9]|session[_-]?token...'`, minus doc/prose matches) — the only hits are doc-comment prose about the session-token concept and the synthetic test tokens `kimi-synthetic-test-token` / `kimi-synthetic-secret`, which never authenticate anywhere. No real or live credential, cookie, raw header, or account identifier appears in the diff.

## Acceptance-criteria evidence

| PRD criterion | Evidence (real test names, all in `crates/providers/src/kimi/`) |
| --- | --- |
| Fixture tests cover shared-pool metrics and unsupported substitutes | Shared-pool mapping: `membership::tests::parses_shared_pool_from_fixture` (consumed `"30"` → `Value::PercentagePoints`, remaining → `Value::Ratio("0.7")`, `refreshes_at` → `quota_refreshes_at` as `Value::Native { unit: "unix_seconds" }` with the exact epoch `1793491200`; all metrics `Provenance::Observation` + `QueryKind::Unvalidated`, `source_as_of: None`, and every metric passes `validate()`), `mod::tests::success_maps_shared_pool_and_keeps_unsupported_rows`. Missing refresh time omits the metric rather than inventing one: `missing_refresh_time_omits_the_metric`. Unsupported substitutes: `mod::tests::success_maps_shared_pool_and_keeps_unsupported_rows` and `wallet_only_payload_fails_pool_without_substituting` assert the `api-wallet` and `code-limits` datasets always carry exactly one `Availability::Unsupported` / `Reason::Unsupported` row with no value, and that a response carrying only wallet/Code-limit sections fails `shared-pool` with `InvalidResponse` instead of substituting. |
| Explicit refusal to substitute API wallet or independent Code limits | In addition to the adapter-level tests above, `membership::tests::wallet_only_payload_is_invalid_never_substituted` proves the parser only reads `data.shared_pool` and returns `InvalidResponse` for a wallet-only envelope; the parser never inspects the sibling sections. The refusal rows are always present — success, pool failure, and auth failure paths all include them (`forbidden_maps_to_pool_permission_failure`, `oversized_body_fails_pool_dataset_only`). |
| Live reconciliation notes vs official membership page when credentials available | Credentials were not available; see "Live reconciliation blocker" below. The `#[ignore]` live smoke test `quotavista_test_kimi_live_membership_session_token` is runnable exactly when Jonathan supplies a session token locally: `QUOTAVISTA_KIMI_SESSION_TOKEN=<token> cargo test -p quotavista-providers -- --ignored`. It never runs in CI or default `cargo test`, asserts only the `Unvalidated` provenance contract and metric invariants, and prints only error categories and dataset names. This checkbox is deliberately NOT ticked in `prd.md`. |
| No cookies, raw headers, or identifying account data in repository fixtures | All nine fixtures under `crates/providers/src/kimi/fixtures/` contain only invented numeric/string values and RFC 3339 timestamps; no cookie, header, token, or account field exists in any fixture. Secret scan above. Redaction asserted by tests: `bearer_token_debug_and_display_are_redacted` and `adapter_debug_redacts_auth`. |

Additional invariants covered beyond the PRD boxes:

- Exact decimal accounting: percentage literals parsed from raw JSON text (string or bare number), never through `f64`: `rejects_signed_exponent_and_escaped_percents` (signs, exponents, objects, null, `.5` all rejected), `precision_violation_is_invalid` (>16 fractional digits rejected — capped at 16 so the derived ratio always fits the core 18-digit decimal domain exactly), `remaining_ratio_is_exact_across_scales` (`30`→`0.7`, `33.5`→`0.665`, `99.995`→`0.00005`, `100`→`0`, `0`→`1`, 16-digit precision preserved). Subtraction is manual coefficient alignment because core `Decimal` has no `checked_sub` and is nonnegative-only.
- Overconsumption honesty: `consumed_percent` above `100` is a malformed response, never clamped: `overconsumption_is_invalid_not_clamped` and the `remaining_ratio` error assertions.
- No invented token totals: the dataset's `tokens` metric is always `Missing`/`NoData` with no value — percentage points cannot yield token totals: `tokens_metric_is_missing_never_invented`.
- Refresh-time honesty: `quota_refreshes_at` is emitted only when the source exposes `refreshes_at`; a bad timestamp fails the dataset: `bad_refresh_timestamp_is_invalid`. The value travels as `Value::Native { unit: "unix_seconds" }` because the domain has no timestamp value shape; it stays `Unvalidated` until reconciliation.
- Error contract at the port boundary (mirrors P2-02): missing credential → whole-call `PermissionRequired` before any network traffic with the fake transport recording 0 calls (`missing_token_fails_whole_call_without_network`); transport offline/timeout → whole-call `Offline`/`Timeout` (`transport_offline_fails_whole_call`, `transport_timeout_fails_whole_call`), preserving last-successful values because failures never emit overwrite rows; 401/403 → pool-dataset `PermissionRequired` only (`forbidden_maps_to_pool_permission_failure`, `unauthorized_maps_to_pool_permission_failure`); 429 → whole-call `RateLimited` with the shared `parse_retry_after` cooldown (`rate_limit_carries_cooldown_from_retry_after_seconds` at 120 s, `rate_limit_without_header_uses_default_cooldown` at 60 s); oversized/non-UTF-8/malformed bodies → pool-dataset `InvalidResponse` only (`oversized_body_fails_pool_dataset_only`, `non_utf8_body_fails_pool_dataset_only`, `malformed_success_body_fails_pool_dataset_only`); any other status (503, 404) → whole-call `InvalidResponse`, never `Offline` (`server_error_fails_whole_call_invalid_response`).
- History safety: `history_is_not_served_yet` asserts `history_page` returns `Ok(None)` with and without a cursor, so committed history stays untouched instead of serving unvalidated bucket shapes.
- Transport hygiene: `transport::tests::base_url_must_be_https` (http:// and file:// rejected), `transport::tests::transport_debug_shows_base_url_only`; the HTTPS-only/no-redirect/30 s-timeout/256 KiB streaming-cap guarantees are inherited by construction because `ReqwestKimiTransport` wraps the shared `ReqwestTransport`.

## Release-criteria status (honest mapping)

- A01 (real adapters retrieve core data): **not PASS** — that requires live reconciliation evidence. This skeleton's fixtures-only state is the P2-03 deliverable as planned; the live path is implemented and env-gated but unexercised.
- A02 (API wallet / independent Code limits never substitute for membership quota): code-complete and tested offline (`wallet_only_payload_fails_pool_without_substituting`, `wallet_only_payload_is_invalid_never_substituted`, persistent `Unsupported` rows); live confirmation pending a session token.
- A03 (reconcile vs official membership page): **not PASS** — blocked on locally supplied auth; see below.
- A06 (offline/timeout/429/permission failures preserve data + typed errors): satisfied at the adapter boundary — typed `ProviderError` variants throughout; failures return errors or `Failed`/`Unsupported` rows, never overwrites with zeros.
- A07 (freshness): partial — `observed_at` records the local observation instant; `source_as_of` stays `None` until reconciliation confirms a server-side timestamp field in the membership envelope (no server timestamp is claimed that the endpoint did not vouch for); flagged TODO for reconciliation.

## Live reconciliation blocker

**No Kimi session token was available during this task, so no live Kimi call has been made.** Wire shapes in `crates/providers/src/kimi/fixtures/*.json` are sanitized inventions and must be confirmed against the real membership page before any acceptance criterion that needs live evidence (A01/A03 PASS) is claimed. Specifically pending:

1. Confirm the real origin (`https://www.kimi.com` placeholder in `DEFAULT_BASE_URL`) and the membership endpoint path (`/api/membership/shared-pool` placeholder in `MEMBERSHIP_PATH`), including how the session is presented (Authorization bearer vs cookie-derived token) within the user-authorized connection flow.
2. Confirm field spellings/casing (`data.shared_pool.consumed_percent`, `data.shared_pool.refreshes_at`) and whether the percentage arrives as a string or bare number (both are accepted; strictness stays).
3. Confirm whether the source exposes used/remaining in any unit beyond percentage points; percentage points stay percentage points — no token total is ever derived.
4. Confirm whether the envelope carries a server timestamp; if so, map it to `source_as_of` (currently `None` — never a fabricated instant) during reconciliation.
5. Confirm the real 401 vs 403 distinction for an expired/insufficient session, and the real 429 `Retry-After` shape.
6. Run the live smoke test locally with the token and record results here; promote `QueryKind::Unvalidated` → validated classification only through the capability-validation process, not in the adapter.

The adapter is strictly read-only: exactly one GET to the membership path per `current()` call, nothing else. Session-token delivery at runtime is a higher-layer concern (native secret store / session memory through `SessionTokenSource` after an explicit user-authorized connection flow); the adapter never scans browser cookies, never reads environment variables in library code — the documented env var is read only inside the `#[ignore]` test.

## Review boundaries

Local-first: to be committed on `jondong/p2-03-kimi-adapter` in this worktree; no push, no PR. Independent review (different provider/model than the implementer) is the coordinator's next step. P2-01 capability reports remain missing on disk (soft dependency folded into module docs); refresh scheduling (P2-04) and rotation (P2-05) remain open for later tasks and were not started.
