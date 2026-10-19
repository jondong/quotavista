# P1-02 validation evidence

## Scope and prerequisite

- P1-01 was confirmed completed in `.trellis/tasks/archive/2026-10/10-18-p1-01-workspace/task.json` before activation. Git history contains PR #1's merge (`add75be`) and the subsequent archive commit (`cb7ad81`); `gh pr view 1` reported `MERGED`.
- Resolved activation syntax with `python3 .trellis/scripts/task.py start --help`, then ran `python3 .trellis/scripts/task.py start 10-18-p1-02-contracts`. This worker owns only this task and dispatched no sub-agents.
- Environment: macOS arm64, Rust 1.99.1, Node v24.22.0, npm 11.19.1. Tests use synthetic in-memory values and throwaway filesystem fixtures only. No database or live provider was opened; no real credentials or identifying account data were used.
- Task state stays in progress pending maintainer review. It is neither archived nor merged by this worker. No changes are pushed to main.

## PRD evidence

| Criterion | Executable evidence | Outcome |
| --- | --- | --- |
| Exact counts and decimals | `number::tests::token_extremes_and_subcounts`, `decimals_reject_without_rounding`, `ratios_and_money_stay_unit_safe`; `exact_json_round_trips_do_not_pass_through_js_numbers` | u64 facts, checked u128 totals/overflow, counts above 2^53 and through u128::MAX survive JSON string round trips. Invalid syntax, excess scale/significant digits, ratio bounds, and currency mismatch reject without rounding. |
| Independent value/state semantics | `domain::tests::failure_does_not_erase_cached_zero_or_partial_coverage`, `alert_eligibility_requires_current_source_evidence`; `mapping_preserves_null_zero_units_time_and_independent_failure` | Zero, null, unsupported, missing, partial coverage, stale retained timestamps, active lifecycle and local dataset failure remain independent. Base token totals do not blindly add cached/reasoning details. |
| Calendar boundaries | All five `calendar::tests` | 23/25-hour DST days, Monday anchors, inclusive-to-half-open conversion, single-as-of clipping, midnight's absent effective interval, next millisecond, reversed/future/>366-day ranges, unknown zones, invalid dates and ambiguous/nonexistent midnight policy pass. Whole source buckets cannot be prorated into incompatible boundaries. |
| Checked transport contract | All thirteen `crates/contracts/tests/contracts.rs` tests; generated TypeScript; CI drift step; frontend typecheck with canonical request fixtures | All eleven requests decode/validate and all eleven result envelopes round-trip with explicit tags/nulls. Locale preference and safe typed errors are represented. Invalid fields/providers/IDs/revisions/dates/secret line breaks and bounds reject; accepted secrets retain bytes and invalid-secret errors do not echo input. Domain types have no transport derives. Repeated generation is identical; modified and missing output fail check mode. |

## PR #2 review follow-up

| Review item | Fix and regression evidence |
| --- | --- |
| 1. SafeError taxonomy | Private code/retryability/message with read-only getters and constructor-based checked deserialization. `safe_errors_reject_taxonomy_tampering` covers every code plus mismatches, missing fields, duplicate fields, and unknown fields. Compile-fail doctests prevent external field mutation. |
| 2. Metric value invariants | Private unit/value pair; construction and decoding validate canonical u128 tokens, decimal amounts, and bounded ratios. `metric_values_are_checked_by_unit_at_the_boundary` covers every unit, u128::MAX, zero/null distinction, overflow/precision/canonicality rejection, duplicates/unknowns, and constructor failures. Compile-fail doctests protect both fields. |
| 3. Local mapping errors | `internal_error` with fixed safe prose identifies invalid domain values, IDs, zones, periods, or source labels. `local_mapping_failures_use_internal_error_not_provider_error` checks the code/message and preserves `invalid_response` for provider bodies. |
| 4. Alert classification | Explicit core `QueryKind` separates validated current, unvalidated, historical, delayed-historical, and stale-cache queries. Core and mapping tests reject historical/delayed/cache/NotYetSettled/expired/stale candidates but preserve eligibility under SyncPending alone. Available/null/no-value contradictions reject. |
| Nits | `overview_has_no_grain_while_statistics_requires_it` checks separate overview/statistics inputs; TypeScript request fixtures mirror them. `source_metadata_is_bounded_without_truncating_identity` checks the 128-code-point SourceLabel boundary for native units, source period keys, and authorization categories. Phase-one task inventory no longer claims every checkbox is pending. |

No handlers or alert delivery were added. Query classification remains the future provider capability layer's responsibility, not an inference from collection time. All follow-up fixtures remain synthetic and database-free.

## Commands and results

Commands run from the repository root unless indicated otherwise:

| Command | Result |
| --- | --- |
| `cargo fmt --all` | Applied formatting. |
| `cargo fmt --all -- --check` | Pass. |
| `./scripts/check-core-boundaries.sh` | Pass: only pure calendar direct dependencies, across all targets/features and dependency kinds. |
| `./scripts/test-check-core-boundaries.sh` | Pass: clean core, forbidden ordinary/optional/inactive-target dependency, and Cargo-error fixtures. |
| `cargo test --locked -p quotavista-core -p quotavista-contracts` | Pass; rerun as part of the full workspace below after review changes. |
| `cargo run --locked -p quotavista-contracts --bin generate-contracts` | Pass: generated `apps/desktop/src/generated/contracts.ts`. |
| `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` | Pass: committed-candidate output equals generation. Idempotence and negative drift/missing-output cases also pass in the generator integration test. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Pass. Initial large-envelope-variant lint was fixed by boxing the result, without changing JSON/TS shapes. |
| `cargo test --locked --workspace` | Pass after the PR #2 follow-up: 10 core tests, 13 contract integration tests, 3 existing desktop/infrastructure/provider tests, and 4 compile-fail privacy doctests; no failures. |
| `cargo test --locked -p quotavista-contracts` | Pass after adding the missing-output regression assertion. |
| `cargo build --locked -p quotavista-desktop` | Pass (unsigned debug build; not a native interaction test). |
| `cd apps/desktop && npm ci` | Pass; audit reports zero vulnerabilities. |
| `cd apps/desktop && npm run typecheck` | Pass, including generated contracts. |
| `cd apps/desktop && npm run lint` | Pass. |
| `cd apps/desktop && npm run build` | Pass. |
| Active LSP diagnostics on all six Rust/TS source files changed by the follow-up | All six confirmed clean with no diagnostics (the initial short-budget probe was inconclusive for three; the longer probe completed). |
| `git diff --check` | Pass. |

The final commit and PR checks provide the build identity; the PR body records the same commands and links to this evidence. GitHub CI must be green before the worker's final response.

## Review notes and remaining gaps

- Reviewed every new source file, explicit mappings, generated output, dependency/lockfile changes, workflow, boundary fixtures, task artifacts, and affected specs. Kept existing desktop/provider callers compatible through the core `Provenance` re-export.
- Review corrected exact-midnight handling without allowing invalid/empty actual intervals, and strengthened dependency checks against optional/inactive-target bypasses.
- `ts-rs` reports unsupported serde attributes (`try_from`, `into`, `deny_unknown_fields`). No diagnostics are suppressed: scalar string aliases match real JSON serialization and validation tests, while Rust retains runtime validation/unknown-field rejection. TypeScript declarations do not validate untrusted data.
- Existing frontend tooling reports an ESLint deprecation and npm install-script notices; installation, lint, typecheck and build still pass. No dependency upgrade outside this task was made.
- Command registration/authorization, runtime revision/entity checks, network/body limits, persistence, native interaction, provider access, full statistics aggregation and bilingual UI remain deferred. Synthetic tests are contributions toward A05/A13/A14/A17/A18/A22/A25, not release-acceptance closure. No production/business database access occurred.
