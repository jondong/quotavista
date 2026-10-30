# P2-01 validation record

## Scope and prerequisites

Task: `p2-01-capability-fixtures` (`.trellis/tasks/10-29-p2-01-capability-fixtures/`),
parent `10-29-phase-two-provider-validation`, dependency `10-29-p2-00-plan`
(archived). Baselines: requirements 0.9, architecture 0.4.

Activation, run in this worktree before any task work:

```sh
export TRELLIS_CONTEXT_ID="quotavista-p201-020c7b13-e9f8-4566-b264-554f19d31aa1"
python3 .trellis/scripts/task.py validate 10-29-p2-01-capability-fixtures
python3 .trellis/scripts/task.py start 10-29-p2-01-capability-fixtures
```

`validate` passed (`implement.jsonl` 6 entries, `check.jsonl` 7 entries); `start`
reported `✓ Status: planning → in_progress` and
`✓ Branch recorded: jondong/p2-01-capability-fixtures`. The context identifier is a
local runtime pointer and is deliberately not committed.

Environment: macOS 27.0.1 arm64, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`,
`cargo 1.99.1 (5f94df478 2026-08-27)`, `node v24.22.0`. `npm ci` in
`apps/desktop` added 239 packages (eslint 9.39.5, vitest 5.0.5, vite 7.3.7).

This task is documentation and tooling only. It changes **no Rust source file**,
adds **no dependency**, and touches nothing under `crates/` or `apps/`. It contacts
**no provider endpoint**, uses **no credential**, and opens **no member's
subscription page**. Every value in every fixture is invented.

### Parallel-ownership fence

P2-03 (Kimi adapter) ran concurrently with this task and owns
`crates/providers/src/kimi/**`, the kimi module wiring in
`crates/providers/src/lib.rs`, and `crates/providers/Cargo.toml` dependency changes.
None of those paths was read for modification or written by this task. P2-03 has since
landed (`d2f0b28`) and this branch was rebased onto it, so `crates/providers/src/kimi/`
does exist at the current HEAD — it arrived through the rebase, not through this task.
Verified at the end of the work:

```text
$ git status --porcelain | grep -E ' (crates|apps)/'
(no output)
```

No file under `crates/` was written. The OpenRouter adapter-local fixture directory
`crates/providers/src/openrouter/fixtures/` was left exactly as P2-02 landed it,
including `activity_shape_only.json`, whose schema mismatch is documented rather than
repaired (see "A schema defect found and documented" below).

## Validation status

Every command below was run in this worktree at the recorded HEAD. Exit codes are
real; where a code could not be captured the reason is stated.

| # | Command | Result |
| --- | --- | --- |
| 1 | `./scripts/check-core-boundaries.sh` | rc=0 — `quotavista-core calendar-only dependencies: boundary OK` |
| 2 | `./scripts/test-check-core-boundaries.sh` | rc=0 — 5 regression checks ok, `all boundary-gate regression checks passed` |
| 3 | `cargo fmt --all -- --check` | rc=0, no diff |
| 4 | `cargo clippy --locked --workspace --all-targets -- -D warnings` | `Finished dev profile`; `grep -cE '^error'` over a `--message-format=short` re-run = **0** |
| 5 | `cargo test --locked --workspace` | rc=0 — **251 passed, 0 failed, 2 ignored** |
| 6 | `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` | rc=0 — `TypeScript contract is current` |
| 7 | `node scripts/check-validation-fixtures.mjs` | rc=0 — `validation fixtures ok: 23 JSON bodies, 1 other fixture(s), 24 provenance files, 9 secret patterns clean, 2 reports cross-checked` |
| 8 | `(cd apps/desktop && npm run lint)` | rc=0, no findings |
| 9 | `(cd apps/desktop && npm run i18n:check)` | rc=0 — `Catalog parity OK: 217 keys, placeholders matching in both catalogs` |
| 10 | `(cd apps/desktop && npm run test)` | rc=0 — `Test Files 8 passed (8)`, `Tests 82 passed (82)` |
| 11 | `(cd apps/desktop && npm run typecheck)` | rc=0 |
| 12 | `(cd apps/desktop && npm run build)` | rc=0 — `✓ 44 modules transformed` |

Per-suite test counts for step 5, all `0 failed`:

| Suite | Passed | Ignored |
| --- | --- | --- |
| `quotavista-contracts` integration | 16 | 0 |
| `quotavista-core` lib | 31 | 0 |
| desktop lib (Rust) | 82 | 0 |
| `quotavista-infrastructure` | 66 | 1 (native store) |
| `quotavista-providers` | 43 | 1 (`quotavista_test_openrouter_live_credits_management_key`, requires a live Management Key) |
| `quotavista-contracts` doctests | 13 | 0 |
| **Total** | **251** | **2** |

These totals are unchanged from the P2-02 close, which is the expected result: this
task modifies no Rust.

Three notes on output capture and noise:

- Steps 4 and 6 emit `warning: failed to parse serde attribute … ts-rs failed to
  parse this attribute. It will be ignored.` for `try_from`, `into`, and
  `deny_unknown_fields`. These come from the `ts-rs` macro expansion in
  `quotavista-contracts` and are present at the base commit `e665d11`.
- An earlier background run of this battery was started **before**
  `scripts/check-validation-fixtures.mjs` existed, so its step 7 failed. That run's
  numbers are not cited anywhere in this record; every figure above comes from the
  single fresh run after all files were in place.
- Steps 8–12 were run twice. The first run needed `npm ci` because this worktree had
  no `apps/desktop/node_modules`; its retained log holds the output text quoted above
  (`Catalog parity OK: 217 keys`, `Test Files 8 passed (8)`, `✓ 44 modules
  transformed`) but records only `install_rc=0`, because the background activation
  shell is POSIX `zsh -c` and does not expand `${PIPESTATUS[0]}`. Per-step exit codes
  were therefore re-captured by a second run under `bash`, one command per line with no
  pipe, which is where every `rc=0` in rows 8–12 comes from. Build duration is
  deliberately not cited: it varies between the two runs and is not a pass/fail fact.

`markdownlint-cli2` (v0.24.0 / markdownlint v0.42.0) is **not** part of the
documented battery in `CONTRIBUTING.md`, and the repository does not enforce
`MD013` line length (`docs/architecture.md` has 160 lines over 80 characters,
`docs/traceability.md` 148). Run with `MD013` and `MD033` disabled over
`docs/validation/**/*.md`, `docs/traceability.md`, `docs/phase-two-plan.md`, and this
task directory, it reports 2 issues, both `MD012` and both **pre-existing at
`HEAD`**, confirmed by linting `git show HEAD:…` copies:

- `docs/traceability.md:74` (this task's diff touches lines 13 and 149+ only)
- `docs/validation/native-target-procedure.md:233` (untouched by this task)

All files this task adds — both capability reports, `docs/validation/fixtures/README.md`,
`docs/validation/fixtures/kimi/subscription_page_observed_shape.md`, and this record —
lint clean.

## Secret sweep

Two independent sweeps were run.

1. The gate itself: `scripts/check-validation-fixtures.mjs` applies its nine
   secret-like patterns to every file under `docs/validation/fixtures/` **and** to
   both capability reports. Result: clean (step 7 above).
2. A wider one-off sweep over every file this task adds or modifies — the 58 paths in
   `git diff --name-only 877536d...HEAD` — using the same nine patterns plus AWS
   access-key IDs (`AKIA…`), GitHub tokens (`ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`), and
   Slack tokens (`xox[baprs]-…`): **0 hits**. The first run at the delivery commit
   covered 55 files; it was re-run at the fix-round HEAD over all 58 (the three added
   since are this record and the two Trellis workspace files), still **0 hits**.

```text
files scanned: 58
TOTAL HITS: 0
```

Two benign near-misses were examined and cleared rather than ignored:

- `docs/traceability.md:215` originally read "bearer and basic authorization
  values", which a naive `(bearer|basic)\s+\w{12,}` scanner matches on the word
  `authorization`. The bullet was reworded to "a bearer or basic credential carried
  in an `Authorization` header" so the phrase no longer resembles a header value.
- `docs/validation/provider-capability-openrouter.md:170` quotes the provider's own
  masked example label `"sk-or-v1-au7...890"`. The ellipsis breaks the 20-character
  run the pattern requires, so it does not trip the gate. Fixture `label` values use
  `synthetic-key-…` and never the provider's real prefix.

`git check-ignore -v` confirms `apps/desktop/node_modules` and `apps/desktop/dist`
(created by steps 8–12) are ignored by `.gitignore:20-21`, so the dependency install
left nothing committable behind.

## Acceptance-criteria evidence

| PRD criterion | Status | Evidence |
| --- | --- | --- |
| OpenRouter capability report distinguishes Management Key Credits, key budget, Activity (last 30 UTC days), and Analytics; marks unsupported metrics explicitly | **Met** | `docs/validation/provider-capability-openrouter.md` (559 lines). Section "The four surfaces are not interchangeable" gives each surface its own subsection: `GET /api/v1/credits` (Management Key; `total_credits`/`total_usage` only), `GET /api/v1/key` (explicitly **not** management-only), `GET /api/v1/activity` (the last 30 *completed* UTC days), and `POST /api/v1/analytics/query` with `GET /api/v1/analytics/meta`. A 22-row capability matrix records authorization category, granularity, historical range, and reporting zone per capability, as `docs/architecture.md:156-185` requires. The section "Required metrics that no documented surface can satisfy" names them explicitly: a settled total for today, top-up and fee history, a source-supplied as-of instant or currency on `/credits`, and a stable account identity for `provider_scope_id`. |
| Kimi capability report covers shared membership pool, refresh timing, history limits, and what cannot substitute (API wallet, independent Code limits) | **Met** | `docs/validation/provider-capability-kimi.md` (438 lines). "The shared membership pool" records what shares it and that CLI, VS Code, desktop and third-party requests all count once; "Refresh timing" records monthly refresh by **subscription cycle, not calendar month**, with the documented worked example (subscribe 3:00 PM December 1 → next refresh 3:00 PM January 1, "check your subscription page for the exact time"); history limits are recorded as the most recent **1000** usage-detail records plus the official delay warning "usage-detail data may be briefly delayed; treat the current quota display as authoritative". "Three products, three wallets — only one of them is in scope" quotes the public statements that the Open Platform is pay-as-you-go "with no subscription plans" and that "the Kimi Code API and the API services provided by this platform are mutually independent", and the report states plainly that neither the API wallet nor the independent Kimi Code rolling-5-hour and legacy weekly limits may substitute for membership quota. |
| Fixture corpus builds and is referenced by adapter tests; contains no secrets | **Partially met — deliberate deviation, see below** | Corpus: 49 files under `docs/validation/fixtures/` (23 JSON bodies, 1 Markdown page description, 24 sibling provenance files, 1 README). "Builds": every JSON body parses and every provenance file cross-checks, asserted by `node scripts/check-validation-fixtures.mjs` (rc=0). "No secrets": two sweeps, 0 hits. "Referenced by adapter tests": **not** satisfied for this corpus. |

### Deviation on "referenced by adapter tests"

The corpus is referenced by `scripts/check-validation-fixtures.mjs`, a
dependency-free gate, not by a Rust adapter test. Three reasons, in order of weight:

1. **No Kimi adapter existed to reference it when this task ran.** P2-03 owns
   `crates/providers/src/kimi/**` and was running concurrently. Half the corpus is
   Kimi shapes; wiring them into adapter tests was P2-03's decision, not P2-01's.
   *State at the fix-round HEAD:* P2-03 has since landed (`d2f0b28`) and this branch
   was rebased onto it. It shipped its own adapter-local fixtures under
   `crates/providers/src/kimi/fixtures/`, and `crates/providers/src/kimi/mod.rs:9-11`
   records that "no capability reports or canonical fixtures under `docs/validation/`
   existed when this module was written". The two Kimi corpora therefore coexist; see
   the follow-up note below.
2. **Wiring the OpenRouter half would mean editing a crate under concurrent
   ownership.** A test module would require a `mod` line in
   `crates/providers/src/openrouter/mod.rs`; if `serde_json` were not already visible
   to a `tests/` target, the fix would land in `crates/providers/Cargo.toml`, which
   the fence assigns to P2-03.
3. **The task instruction authorized exactly this fallback**: "Smoke test or script
   that asserts fixtures parse / contain no secret-like patterns if practical without
   fighting P2-03; otherwise document a grep checklist and run it."

What the gate does assert, so the substitution is not merely weaker prose:

- every file is valid UTF-8, at most 64 KiB, and every `.json` file parses;
- every fixture has a sibling provenance file and vice versa, in both directions;
- each provenance file carries all 12 required keys, names an existing report, and
  honestly declares `sanitized: true`, `validated: false`, and
  `captured_from_live_account: false` — a meta file claiming validation fails the gate;
- no fixture body carries a top-level `_`-prefixed comment key, so bodies stay
  byte-exact wire JSON a future adapter can parse directly;
- none of the nine secret-like patterns matches any corpus file or either report;
- the Kimi userinfo fixture's `nickname`, `userLevelName`, `domain`, `domainName`,
  `email`, `phone.countryCode`, and `phone.number` are exactly the sentinel
  `<redacted-at-capture>`, and `avatar`, `bio`, `globalId`, `username`,
  `createdTime`, `lastLoginTime` are absent entirely;
- every `request_id` is a 26-character ULID-shaped string;
- every `fixtures/…` link in the capability reports resolves on disk;
- every fixture is cited by at least one report, and fixture basenames are unique, so
  a file cannot be added, renamed, or orphaned without the gate noticing.

The adapter-local OpenRouter fixtures from P2-02 **are** referenced by adapter tests,
and the report's fixture table now names the exact test and line for each file rather
than a vague description. Verified against the source, not recalled:

| Fixture | Test (file:line) |
| --- | --- |
| `credits_ok.json` | `parses_string_amounts_from_fixture` (credits.rs:245), `credits_success_maps_money_metrics` (mod.rs:338), `key_budget_is_reported_unsupported_not_conflated` (mod.rs:357) |
| `credits_number_amounts.json` | `parses_bare_number_amounts_exactly` (credits.rs:263) |
| `credits_overdrawn.json` | `negative_remaining_is_reported_missing_not_clamped` (credits.rs:276) |
| `credits_missing_field.json` | `missing_amount_field_is_invalid` (credits.rs:291) |
| `credits_precision_violation.json` | `precision_violation_is_invalid` (credits.rs:300) |
| `credits_malformed.txt` | `malformed_body_is_invalid` (credits.rs:309), `malformed_success_body_fails_credits_dataset_only` (mod.rs:509) |
| `credits_forbidden.json` | `forbidden_maps_to_dataset_permission_failure` (mod.rs:384) |
| `credits_rate_limited.json` | `rate_limit_carries_cooldown_from_retry_after_seconds` (mod.rs:424) |
| `activity_shape_only.json` | none — named only in a `history_page` doc comment (mod.rs:241) |

Line numbers are the `fn` declaration lines in `crates/providers/src/openrouter/` at
commit `1f689e1`; an earlier draft cited the `include_str!` line one below each `fn`.
Test **names** are the durable identifier — the lines drift whenever the crate is
edited.

Recommended follow-up for the coordinator: P2-03 has since landed and shipped its own
adapter-local fixtures under `crates/providers/src/kimi/fixtures/` instead of consuming
`docs/validation/fixtures/kimi/`, so criterion 3's "referenced by adapter tests" is
still open for the Kimi half of this corpus. Reconciling the two Kimi fixture sets —
deciding which shapes are canonical and whether the adapter should read
`docs/validation/fixtures/kimi/` — spans two tasks and is a coordinator decision, not
one P2-01 may make under the ownership fence.

### A schema defect found and documented, not silently repaired

`crates/providers/src/openrouter/fixtures/activity_shape_only.json`, landed by P2-02
and self-labelled UNVALIDATED there, uses field names that do not match the published
Activity schema. The stub carries `id`, `day`, `api_key`, `input_tokens`,
`output_tokens`, `cache_read_tokens`, `cache_write_tokens`, and `cost`; the documented
endpoint returns `date`, `prompt_tokens`, `completion_tokens`, `cached_tokens`,
`reasoning_tokens`, `usage`, `requests`, `model`, `model_permaslug`, `endpoint_id`,
`provider_name`, and `byok_usage_inference`.

This task documents the mismatch in a dedicated report section and publishes corrected
shapes at `docs/validation/fixtures/openrouter/activity_ok.json` and
`activity_grouped_by_workspace.json`, instructing whoever implements Activity parsing
to supersede the stub. It does **not** edit the stub: the file is named in a
`history_page` doc comment in `mod.rs`, and correcting it would mean editing Rust in a
crate that was under P2-03's concurrent ownership when this task ran, for no benefit to this task's deliverables.

## Release-criteria status (honest mapping)

No release acceptance criterion is closed by this task. Nothing below is a PASS.

| Criterion | Status after P2-01 | Why |
| --- | --- | --- |
| A01 Provider adapters and capability reports | **Pending** (G01/G02 open) | Capability surfaces are now written down and separable, which is the *precondition* for reconciliation, not reconciliation. No live OpenRouter Credits, Activity, or Analytics response was retrieved; no Kimi membership quota was retrieved by any path. Native units and timestamps were never compared against a real account. |
| A02 Scope and product identity | **Pending** | The one-pool rule and the substitution ban are documented with public sources and the `oauth/usage` window set is identified as exactly where an accidental substitution could occur. Not observed against a real account. |
| A03 Kimi adapter validation | **Pending** (G01 open) | The three facts a member-page comparison needs are recorded — subscription-cycle refresh, percentage-only remaining quota, the 1000-record history bound, and the official delay warning. The comparison itself, the allowed display precision, and the retrieval path all remain open. |
| A04 Metric scope and permission state | **Pending** | Account-versus-key field semantics are documented exactly (`null` means unlimited, not unknown; `remaining_usd = limit_usd - spend_usd`; `budgets[]` arrives in enforcement order so summing double-counts). The shipped adapter still reports the key-budget dataset as `Unsupported`, so nothing is summed in code today. No live permission state was observed. |
| A05 Metric transport and UI states | **Pending** | The corpus adds documented shapes for unsupported, insufficient-scope, expired-credential, rate-limited, upstream-error, truncated, and server-timeout states, which supports future testing. No real zero, null, stale, or failed state was observed. |
| A10 / A24 | **Pending** | The fixture gate is new evidence for secret hygiene; the release documentation audit A24 asks for remains pending. |

`docs/traceability.md` was updated by appending to the four status cells (A01–A04) in
the existing `Pending; …` style already used by A10, and by adding a new
`## P2-01 capability and fixture evidence` section before `## Release evidence rules`.
**No `Pending` was converted to `Passed`.** The A05–A25 rows are otherwise untouched.
`docs/phase-two-plan.md` gained a `## Capability reports and sanitized fixtures (P2-01)`
section; its task-inventory table row was deliberately left at `Planning` because that
table carries the note "The table records the initial plan, not live task status."

The Phase 1 exit assessment was not rewritten, no Ubuntu 26.04 GNOME row was invented
as OBSERVED, and no task was archived. P2-04 and P2-05 were not started.

## Live reconciliation blocker

Two blockers stay open and are the reason nothing above reads PASS.

**OpenRouter (G02).** Reconciling `/credits` against the console, confirming the
accounting scope of `total_credits`/`total_usage`, and observing a real permission
verdict all require a Management Key. None is present, and none may be committed. A
Management Key is administrative: the same credential that unlocks Credits, Activity,
and Analytics also unlocks destructive `POST|DELETE|PATCH /api/v1/keys`, BYOK,
guardrail, and organization routes, so the read-only allow-list has to be enforced in
code before a live key is used. The ignored test
`quotavista_test_openrouter_live_credits_management_key` is the existing entry point
and reads `QUOTAVISTA_OPENROUTER_MANAGEMENT_KEY` only inside that test.

**Kimi (G01).** The membership pool has **no documented public JSON API**. The
authoritative source is a login-gated subscription page. The single machine-readable
candidate found — `GET /api/v1/oauth/usage` on the Kimi Code CLI's local server at
`http://127.0.0.1:58627` — belongs to another tool's runtime, needs that tool's bearer
token, is explicitly labelled experimental ("endpoints, fields and event types may
change in any version"), and returns `usedRatio` as a 0–1 float from which tokens must
never be derived. `docs/requirements.md:132-138` forbids reading another tool's
credentials by default, so this path requires an explicit user-authorized connection
flow that does not exist yet. Whether it agrees with a real member's subscription page
is untested.

Also unresolved and recorded as open questions rather than decided: the `usages`
container shape is ambiguous in the public documentation (an object keyed by window
name versus an array of window objects — the corpus assumes the object form and says
so in the provenance notes), and the public Kimi documentation contradicts itself on
tier count (the API overview says "four tiers" while the membership overview lists
five: Go, Plus, Pro, Max, Ultra), which is itself evidence that plan identity must be
read from the account and never inferred from a quota size.

## Files added or modified

```text
M  .trellis/tasks/10-29-p2-01-capability-fixtures/task.json   (status → in_progress)
M  docs/phase-two-plan.md                                      (new P2-01 section)
M  docs/traceability.md                                        (A01–A04 cells + new evidence section)
A  docs/validation/provider-capability-openrouter.md           (559 lines)
A  docs/validation/provider-capability-kimi.md                 (438 lines)
A  docs/validation/fixtures/README.md
A  docs/validation/fixtures/openrouter/*.json + *.meta.json    (13 + 13)
A  docs/validation/fixtures/kimi/*.json + *.meta.json          (10 + 10)
A  docs/validation/fixtures/kimi/subscription_page_observed_shape.md + .meta.json
A  scripts/check-validation-fixtures.mjs
```

49 files under `docs/validation/fixtures/`; 58 files added or modified in total
(`git diff --name-only 877536d...HEAD`).
Nothing under `crates/` or `apps/`.

## Review round 1 (independent reviewer, REQUEST_CHANGES)

The reviewer found no blocking items and three should-fix items. All three are
addressed below, plus the cheap nits. Nothing in this round changed a release-criteria
status: A01–A04 remain **Pending**.

**Should-fix 1 — stale commit SHA.** The journal and index cited
`9d2d852e4b8f748d0d864d52f43809fdd05c5962`, which the rebase onto `877536d` (after
P2-03 landed) rewrote, so it no longer resolved on this branch. Both
`.trellis/workspace/jondong/journal-1.md` and `.trellis/workspace/jondong/index.md` now
cite `68b54ec`, `4cdbc2c`, and `1f689e1` — the three on-branch commits for this
session, in the short-SHA form every other journal row uses. Verified with
`git merge-base --is-ancestor <sha> HEAD` → YES for all three, and a repo-wide grep for
the three dead SHAs now returns nothing.

**Should-fix 2 — `rate_limit` in `current_key_regular.json`.** The reviewer read this
as an invented sub-shape. It is not: the authoritative OpenAPI 3.1 document published
inline at
`https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key` (re-read
2026-10-30) declares `rate_limit` an object with three **required** properties —
`interval` (string), `requests` (integer), `note` (string) — and marks the whole object
`deprecated: true` with the description "Legacy rate limit information about a key.
Will always return -1.", which contradicts the schema's own example values. Dropping it
would have omitted a required field, so it was **kept** and the provenance was
corrected instead. The same schema re-read found a real defect the review did not:
`data` has **24 required fields**, and all three `current_key_*.json` fixtures were
missing two of them — `organization_id` and `workspace_id` — while
`current_key_management.json` and `current_key_unlimited.json` were also missing
`rate_limit`. All three now carry every required field and no unpublished extra
(asserted by the fix script in both directions), and the invented identifiers were made
valid hexadecimal (`…0000000000g1` → `…0000000000c1`, `…0000000000w1` →
`…0000000000b1`) so a strict UUID decoder accepts them. The three identity cases now
cover the documented null branches: an organization-owned key in a resolved workspace,
a management key with `workspace_id: null`, and an individual account with
`organization_id: null`.

This also invalidated a claim in the OpenRouter report. It said `is_management_key`,
`is_provisioning_key`, `expires_at`, `creator_user_id`, and `allowed_data_regions`
"appear only in the API reference example", so their presence was "**unverified**".
The schema publishes the first four as required properties (`expires_at` is published
and nullable but *not* required), so presence is documented and only the *values*
remain unmeasured. The report's `/key` section, capability-matrix row 22, the
"Required metrics that no documented surface can satisfy" bullet, and open question 6
were corrected accordingly — the account-identity question narrowed from "no field
exists" to "which of `organization_id` / `workspace_id` is authoritative for
`provider_scope_id`, and does either survive credential rotation". Two documentation
conflicts are now recorded rather than silently resolved: `is_free_tier` is described
differently by the limits guide ("whether the user has paid for credits before") and
the schema ("Whether this is a free tier API key"), and `/key` documents only 401 and
500 — **no 403**, unlike `/credits`.

**Should-fix 3 — verdict labels.** `docs/phase-two-plan.md` claimed every capability
verdict reads **Documented** or **Not retrievable**. No matrix cell uses either label.
The paragraph now names the labels the matrices actually use — `Supported,
**Unverified**`, `**Unsupported**` (with its three parenthetical reasons),
`**Unsupported by the source**`, `**Unsupported for our purpose** — must not
substitute`, and `**Partial at best**` — and states plainly that "Supported" means the
provider's documentation publishes the field, never that QuotaVista observed it. No
cell reads **Verified**, **Validated**, **Observed**, or **Measured**.

**Nits applied.** Test citations in both the OpenRouter report and this record now give
the `fn` declaration line (an earlier draft cited the `include_str!` line one below, and
four below for two multi-line call sites) and state that convention, with the test name
as the durable identifier; the two worst offsets were `forbidden_maps_to_dataset_permission_failure`
(384, cited 388) and `rate_limit_carries_cooldown_from_retry_after_seconds` (424, cited
428). The file count is 58, not 55 — the earlier figure excluded this record and the two
Trellis workspace files, and the secret sweep was re-run over all 58 with 12 patterns,
still 0 hits. `docs/validation/fixtures/README.md` now says the `request_id` values are
ULID-*shaped* only and are not valid Crockford-base32, so nothing may sort or
timestamp-decode them. Three statements that the rebase made false were corrected: this
record's "No Kimi adapter exists to reference it", its "when P2-03 lands the Kimi
adapter" follow-up, and its "P2-03 is concurrently working in" — P2-03 has landed, and
it shipped its own adapter-local fixtures under `crates/providers/src/kimi/fixtures/`
rather than consuming `docs/validation/fixtures/kimi/`, so the two Kimi corpora coexist
and reconciling them is a coordinator decision across task boundaries. A missing blank
line before a fenced block (markdownlint MD031) was also fixed.

Left as intentional: `docs/phase-two-plan.md:33` still lists P2-01 as `Planning`,
because that table carries the note "The table records the initial plan, not live task
status"; `task.json` stays `in_progress` and journal session 8 stays marked completed
until the coordinator archives.

**Re-verification after this round** (macOS 27.0.1 arm64, Node v24.22.0, Rust 1.99.1):
`node scripts/check-validation-fixtures.mjs` rc=0 — `validation fixtures ok: 23 JSON
bodies, 1 other fixture(s), 24 provenance files, 9 secret patterns clean, 2 reports
cross-checked`; `./scripts/check-core-boundaries.sh` rc=0;
`./scripts/test-check-core-boundaries.sh` rc=0; `cargo fmt --all -- --check` rc=0;
`markdownlint-cli2` over both reports, the fixtures README, `docs/phase-two-plan.md`,
and this record → **0 issues** (the two remaining MD012 hits are in
`docs/traceability.md:74` and `docs/validation/native-target-procedure.md:233`, both
pre-existing at `e665d11` and untouched by this task); the widened secret sweep over all
58 changed files → **0 hits**. `cargo clippy` and `cargo test` were not re-run: this
round changed no Rust file, verified by
`git diff --name-only 877536d...HEAD | grep -E '^(crates|apps)/'` returning nothing.
