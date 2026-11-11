# P2-06 validation evidence

Recorded on branch `jondong/p2-06-live-evidence-docs` in the Orca worktree
`.orca/workspaces/quotavista/p2-06-live-evidence-docs` (macOS arm64, Rust 1.99.1,
Node 24.22.0). This is a documentation-and-evidence slice: it changes no adapter logic.

**No credential was supplied to this task, searched for, read, printed, or committed. No
`#[ignore]`d live test was executed here. No browser or GUI automation tool was used, so no
live UI verification is claimed anywhere in this record.**

## Commands (run from the worktree root, Rust toolchain per `rust-toolchain.toml`)

- `cargo fmt --all -- --check` — exit 0.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — exit 0; zero lines
  matching `^error` in the log. The 128 `warning: failed to parse serde attribute` lines are
  pre-existing `ts-rs` proc-macro noise ("ts-rs failed to parse this attribute. It will be
  ignored.") and are not clippy lints.
- `cargo run --locked -p quotavista-contracts --bin generate-contracts -- --check` — exit 0, so
  `apps/desktop/src/generated/contracts.ts` still matches the Rust contracts.
- `cargo test --locked --workspace` — exit 0, **297 passed, 0 failed** across 13 `test result: ok`
  lines (16, 31, 85, 80, 72, 13 passed plus 7 empty/doc targets). Run **without** `--ignored`
  and **without** any provider credential environment variable. 3 tests stayed ignored and did
  not run: the two env-gated provider live smokes
  (`quotavista_test_openrouter_live_credits_management_key`,
  `quotavista_test_kimi_live_membership_session_token`) and the macOS Keychain native smoke at
  `crates/infrastructure/src/secrets/tests.rs:683`.
- `cd apps/desktop && npm ci && npm run lint && npm run i18n:check && npm test && npm run typecheck && npm run build`
  — all six steps exit 0. `npm test` (vitest): **8 test files passed, 82 tests passed**, duration
  1.10s. `npm run build`: `✓ built in 491ms`.
- `./scripts/check-core-boundaries.sh` — exit 0 (`quotavista-core calendar-only dependencies:
  boundary OK`), and `./scripts/test-check-core-boundaries.sh` — exit 0 (all five boundary-gate
  regression checks pass). Run for completeness; this task touched no Rust module boundary.
- `node scripts/check-validation-fixtures.mjs` — exit 0: `23 JSON bodies, 1 other fixture(s), 24
  provenance files, 9 secret patterns clean, 2 reports cross-checked`. Run because this task edited
  both capability reports, which that gate cross-checks for cited fixture paths and secret-like
  patterns.

Both batteries were re-run after the final code edit (the two Rust comment/attribute changes), so
the green results above describe the committed tree rather than an earlier state.

## PRD criterion 1 — the user-reported live smoke is recorded, labelled, and limited

Evidence: `docs/validation/provider-capability-openrouter.md`, new section
`## Live-smoke evidence (user-reported)`, which carries the supplied output verbatim inside a
fenced block and a five-row Field/Value table.

- **Run timestamp**: 2026-11-10 21:52 Asia/Shanghai (Beijing), as reported by the user.
- **Command class**: a single-test `cargo test` invocation selecting the `#[ignore]`d
  `quotavista_test_openrouter_live_credits_management_key` by name — hence `73 filtered out`.
- **Environment category**: the user's own machine, one real prepaid OpenRouter account, a
  Management Key supplied through `QUOTAVISTA_OPENROUTER_MANAGEMENT_KEY`. No key material,
  account name, organization id, or figure was shared with this task or committed.
- **Result category**: **PASS as reported** — `ok. 1 passed; 0 failed`, with neither a typed
  whole-call error line nor a per-dataset failure line.
- **Evidence class**: user-reported; **not** independently reproduced here; **not** a numeric
  console reconciliation; **not** a desktop UI validation.

What the run does establish is inferred from the test body in
`crates/providers/src/openrouter/mod.rs`: a whole-call failure would have printed
`live call ended in typed whole-call error`, and a dataset failure would have printed
`live dataset prepaid-credits failed`, so neither occurred. `GET /api/v1/credits` was therefore
called exactly once, returned 2xx, and `credits::credits_metrics` decoded the body — which
requires `data.total_credits` and `data.total_usage` to be present and parseable as decimals —
into three `prepaid-credits` rows (`total_credits`, `total_usage`, derived `remaining`) that each
satisfy `CurrentMetric::validate()` (`crates/core/src/provider.rs:84`). That is real but narrow
evidence for the adapter half of A01.

What it does not establish is recorded in the same section: no figure was printed, so nothing was
reconciled against the OpenRouter console; open questions 1–9 all stay open (the parser accepts
both JSON numbers and strings, so a successful decode cannot even settle question 1); Activity
and Analytics were not exercised, so exactly one live endpoint was contacted; and nothing was
observed about key rotation, rate limits, expiry, permission failures, or overdrawn balances.

## PRD criterion 2 — no read-only or vendor-scoped read-only Management Key claim survives

The misleading paragraph in `docs/validation/provider-capability-openrouter.md` now reads
"**Management keys are dangerous by construction, and no read-only variant exists.**" and states
explicitly that nothing in the reviewed sources documents a read-only Management Key, a scope
parameter, or a permission downgrade, that none may be assumed, and that "query-only" describes
**QuotaVista's own client behaviour** rather than a property the credential carries or a vendor
allow-list that could be requested. Four handling bullets replace the old "enforce the allow-list
in code" sentence: a short-expiry key (expiry is fixed at creation and cannot be extended); kept
only on the user's machine — Keychain or session-only, never in a shell profile, config file,
fixture, log, screenshot, commit, or prompt; revoked as soon as the reconciliation is recorded;
and a query-only allow-list enforced in code, which needs `GET /api/v1/credits` today and lists
`GET /api/v1/key`, `GET /api/v1/activity`, `POST /api/v1/analytics/query` and
`GET /api/v1/analytics/meta` as documented candidates for later slices, with no mutating
management route ever admissible.

The same correction landed in the other places that carried the claim:

- `crates/providers/src/openrouter/mod.rs` — the live-test doc comment now describes a Management
  Key as an administrative credential with no read-only or scope-limited variant, and the
  `#[ignore]` reason changed from `(query-only Management Key)` to
  `(administrative credential: use a short-expiry key and revoke it after the run)`.
- `docs/phase-two-plan.md` — the credential/safety bullet now says Management Keys are
  administrative credentials, that no read-only or vendor-scoped read-only variant exists, and
  that query-only is client discipline plus short-expiry/revoke handling.
- `docs/traceability.md` — the new P2-06 section records the same corrected guidance under G02.

Occurrences deliberately **not** rewritten, each checked:

- `docs/validation/fixtures/openrouter/current_key_management.meta.json` — its note already says
  the read-only allow-list "has to be enforced in code" and asserts no vendor read-only mode, so
  it is consistent with the corrected guidance. Left alone to keep the fixture provenance hash and
  `scripts/check-validation-fixtures.mjs` inputs stable.
- `.trellis/tasks/archive/2026-10/10-29-p2-02-openrouter-adapter/validation.md` and
  `.trellis/workspace/jondong/journal-1.md` — archived evidence and an append-only journal. Both
  are historical records of what the earlier task believed; rewriting them would falsify the
  history. The correction is recorded here and in a new journal entry instead.
- `.pi/workflows/trellis-phase-two.js` — carries the old `MANAGEMENT_KEY` prompt constant, but
  `.pi/` is gitignored, so it is local orchestration tooling and not a repository artifact.
- `docs/requirements.md` and `docs/architecture.md` — inspected and **not** changed.
  `requirements.md:161` already says a Management Key "has administrative capabilities beyond an
  inference key" and to "perform only the required queries", and `architecture.md:174` already
  says "Use a Management Key only for required queries" while keeping account Credits distinct
  from key budget. Neither claims a read-only or scoped Management Key, and neither tells the
  reader to wait on a Kimi session token. Per the minimize-churn instruction they were left
  untouched.

## PRD criterion 3 — Kimi blockers re-worded to an unresolved supported integration

In `docs/validation/provider-capability-kimi.md`:

- The G01 honest-status row is now "**Open blocker — unresolved supported integration**" and says
  the gap is not a token the user still owes QuotaVista, that the adapter base URL and membership
  endpoint stay **Unvalidated** placeholders, that Moonshot Open Platform API keys and Kimi Code
  API keys are not substitutes for membership quota, and that no universal impossibility is
  claimed — Kimi may publish a supported flow later, and the row must then be revised.
- The Path B authorization blocker now states that this is **not** a token QuotaVista asks the
  user to go and find, and that Path B remains an open third-party-owned question rather than a
  pending user input.
- The open-question-10 feasibility-gate paragraph records that P2-06 re-examined the question
  against the listed sources and found **no documented official consumer membership session-token
  issuance path** — no OAuth scope, no API-key product, and no account setting that yields a token
  usable against the membership page — and that the officially documented way to view membership
  quota remains signing in and reading the membership/subscription page. It keeps the existing
  prohibition on a manually entered quota standing in for a working automatic integration.
- The closing paragraph adds that nothing authorizes reading browser cookies or another tool's
  stored credentials to close the gap.

The Path A / Path B analysis, the three Path B blockers, the wire-shape description, and the
capability-matrix row "**Unverified** (no authorized path)" were verified accurate and left
unchanged. `crates/providers/src/kimi/mod.rs` got the matching correction: the live-test doc
comment now leads with "NOT RUNNABLE YET: no supported Kimi integration issues a membership
session token…", and the `#[ignore]` reason changed from
`(user-authorized session token)` to
`unresolved supported Kimi integration: no authorized session-token source (see
docs/validation/provider-capability-kimi.md G01)`.

`docs/phase-two-plan.md` was corrected in the same direction: the status line, the P2-01 closing
paragraph, the exit gate, and the starting point no longer describe Kimi as waiting on auth
material the user supplies, and the new `## P2-06 live-smoke evidence and guidance corrections`
section names the unresolved-integration framing. Kimi was not dropped from scope, and no later
feature work was invented.

## PRD criterion 4 — A01–A04 each have a distinct status, and both parents are unchanged

Statuses as recorded in `docs/traceability.md` (A01–A04 rows plus the new P2-06 section) against
the definitions in `docs/phase-two-plan.md`:

| ID | Status | Why this status and not another |
| --- | --- | --- |
| A01 | **Pending** (partial adapter evidence) | The smoke proves the shipped OpenRouter adapter reached and decoded `GET /api/v1/credits` against one real prepaid account. It printed dataset categories and no figure, so no native unit or timestamp was compared against the console, and Kimi has no demonstrated real retrieval path at all. A01 requires both providers' core data plus a native-unit/timestamp comparison. |
| A02 | **Pending** (unchanged) | Nothing observed that Kimi web/app/Code shared consumption counts once, and no code path substitutes an API wallet or an independent Code limit for membership quota. The smoke touched no Kimi surface. |
| A03 | **Pending** (blocker re-worded) | No official member-page comparison has occurred, and the retrieval path is now described as an unresolved supported integration rather than a missing user token. The documented subscription-cycle refresh, 1000-record history bound and official delay warning are still documentation only. |
| A04 | **Pending** (explicitly not PASS) | The smoke line `live dataset key-budget returned metrics` is the adapter's unconditional `Unsupported` placeholder row carrying its metrics payload — `OpenRouterAdapter::key_budget_unsupported` returns `value: None`, `Availability::Unsupported`, `Reason::Unsupported`, `QueryKind::Unvalidated`, `Provenance::Observation`, and is returned as `DatasetResult::Metrics(vec![…])` on every path. `key_budget_is_reported_unsupported_not_conflated` asserts exactly this. It is not a measured key budget, so account Credits versus key budget remain numerically unreconciled and no regular-key-versus-management-key comparison exists. |

Gate rows: **G01** remains an open blocker, re-worded to "unresolved supported integration".
**G02** moved from "Open blocker — no Management Key is available in this worktree" to
"**Partially exercised, still open**": a Management Key was used once, by the user, for the
Credits path, while token semantics, current-day reporting, Activity/Analytics behaviour,
permission failures and expiry handling all remain unmeasured.

Parent status: `.trellis/tasks/10-29-phase-two-provider-validation/` stays **open** — all seven
"Phase exit acceptance" checkboxes in its `prd.md` remain unchecked, including the OpenRouter
console reconciliation and the Kimi member-page reconciliation. Its `task.json` notes and
`implement.md` were updated so the exit gate reads honestly, and `children[]` now lists
`11-11-p2-06-live-evidence-docs`. The Phase 1 parent `10-18-phase-one-prototype` was **not**
touched: it stays incomplete, its Ubuntu/Linux rows stay UNVERIFIED, and no OBSERVED row was
invented. This task did not archive either parent.

## PRD criterion 5 — desktop path inspected, no live UI claim, remaining human steps written down

Inspected in source (no GUI tool available, and none used):

- Credential kind contract: `crates/contracts/src/lib.rs` `wire_enum!(CredentialKind { ApiKey,
  ManagementKey, SessionToken })` → `apps/desktop/src/generated/contracts.ts:25`
  `export type CredentialKind = "api_key" | "management_key" | "session_token";`, re-verified by
  the `generate-contracts --check` run above.
- Connection form: `apps/desktop/src/screens/ConnectionsScreen.tsx:75`
  `readonly credential_kind: CredentialKind;`, `:116-118` the select bound to the draft,
  `:122` `<option value="management_key">`, `:178` default `"api_key"`, `:300` and `:322` the
  add/rotate payloads.
- IPC surface: `apps/desktop/src/ipc.ts:260` `connect_provider`, `:276` `replace_credentials`,
  `:296`/`:299` `refresh_connections`.
- Backend commands: `apps/desktop/src-tauri/src/connections.rs:315` `connect_provider(…)` and
  `:443` the rotation command both take `credential_kind: quotavista_contracts::CredentialKind`,
  and both currently acknowledge it without branching on it (`:362-364` comment "The credential
  kind is part of the wire contract but the synthetic validation does not differentiate kinds"
  plus `let _ = credential_kind;`, repeated at `:498`). Credential routing is instead decided by
  provider id in `crates/infrastructure/src/refresh/router.rs`, where `ResolvedToken`
  (`:47`) implements both `AuthTokenSource::management_token()` (`:49-53`) and
  `SessionTokenSource::session_token()` (`:55-58`) from the same stored secret, and
  `Resolution::LiveOpenrouter` / `LiveKimi` (`:67-68`) are chosen per provider. `token()` (`:154`)
  resolves the stored reference through `SecretStore` and yields `ResolvedToken(None)` on a
  missing reference, an unreadable store, or invalid material, which degrades to the adapter's own
  `PermissionRequired` with no network request. **Recorded as an observation only** — changing it
  is out of this task's scope.
- Existing automated coverage of that path: `apps/desktop/src/screens/ConnectionsScreen.test.tsx`
  (13 cases, including "submits an add with the typed fields and clears the secret after success",
  "shows the session-only note in needs credentials and offers rotation", "rotates credentials
  with the expected revision and clears the secret", "reflects a failed rotation without
  optimistic success"), `apps/desktop/src/ipc.test.ts` (`RevisionTracker` watermark/gap/obsolete
  cases and `subscribeToChanges`), plus `App.test.tsx`,
  `components/DesktopIntegration.test.tsx`, `i18n/i18n.test.ts`, and the Overview/Settings/
  Statistics screen tests — 8 files, 82 tests, all passing above.
- The running App does **not** read provider credentials from environment variables: the only
  non-test `std::env::var` uses in `apps/desktop/src-tauri/src` are the benchmark knobs,
  `SESSION_ONLY_ENV`, `temp_dir`, and `TRAY_DISABLE_ENV`. The two `QUOTAVISTA_*` provider
  variables are read only inside the `#[ignore]`d smokes. This is why the remaining reconciliation
  must be done through the App UI, not by exporting a shell variable.

### Remaining human desktop numeric-reconciliation steps (not performed here)

1. Build and launch the desktop App from this branch (`npm run tauri dev` inside `apps/desktop`).
2. In Connections, add an OpenRouter connection and select credential kind
   **Management Key**. Create the key in the OpenRouter console with the **shortest available
   expiry**, since expiry is fixed at creation and cannot be extended.
3. Paste the key into the secret field and choose **session-only** storage (or Keychain if a
   durable store is wanted). Never put the key in a shell profile, `.env`, config file, fixture,
   screenshot, commit, prompt, or `~/.quotavista-orchestration/` file.
4. Trigger a refresh for that connection and confirm it completes without a typed permission
   error. If it reports a permission failure, the key lacks the required scope: record that and
   stop — do not substitute an inference key's balance and do not let the UI show an invented
   number.
5. At (as close as possible to) the same instant, read the account's Credits from the official
   OpenRouter console. Record **three values**: the console total, the console usage, and what the
   App displays for account balance and usage. Compare native units and decimals, and note the
   retrieval instant for both sides.
6. Confirm the App keeps **key budget visually distinct from account balance** — the key-budget
   dataset must still render as Unsupported rather than as a number, because the adapter reports
   `Availability::Unsupported` for it unconditionally. Any rendered key-budget figure is a defect.
7. Refresh again after a few minutes and confirm the displayed values and the last-successful /
   freshness metadata both move honestly, and that a failed refresh keeps the previous values
   rather than blanking them.
8. Record the outcome in a Trellis validation artifact as **categories and compared figures**,
   with the run instant, environment, and any display-precision or reporting-delay explanation. Do
   not paste the key, the account name, or the organization id.
9. **Revoke the Management Key** in the OpenRouter console as soon as the record is written, and
   remove the connection if the credential was stored durably.
10. Kimi: **skip** the desktop live-connection attempt for membership quota until a supported
    integration exists. Adding a Kimi connection today exercises only the synthetic path, which
    must stay marked `Provenance::Synthetic`.

Until steps 5–7 are performed and recorded, A01 and A04 remain Pending regardless of the smoke.

## PRD criterion 6 — negative constraints honored

No secret or identifying account data was added to git, Trellis artifacts, prompts, logs,
fixtures, or `~/.quotavista-orchestration/`. No credential was hunted for: no environment
variable, shell history, cookie store, or Keychain entry was read, and the `#[ignore]`d live tests
were never executed. No extra provider, endpoint, or feature was implemented, and no `prod/`
path exists or was created. The two Rust edits are a doc comment and an `#[ignore]` reason string
in each of `crates/providers/src/openrouter/mod.rs` and `crates/providers/src/kimi/mod.rs`; no
control flow, signature, or contract changed, which the unchanged 297/0 test result confirms. No
force-push, rebase, reset, or destructive cleanup was run; the orphan Orca terminal
`term_4031fed9` and everything under alpha-console were left untouched. No push, PR, merge, or
archive was performed. All new and edited text is English.

## Open blockers after this task

- **OpenRouter console reconciliation** (A01, A04, G02): owed as the human desktop procedure above.
- **Kimi retrieval path** (A01–A03, G01): unresolved supported integration; needs a product-owner
  decision on Path B or a supported Kimi flow before any adapter validation is possible.
- **Activity / Analytics** (G02): documented candidates only; never exercised against a live
  account.
- **Phase 2 parent exit gate**: open. **Phase 1 parent**: open and incomplete, Ubuntu waived and
  still UNVERIFIED.
