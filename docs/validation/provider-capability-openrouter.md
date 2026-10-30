# OpenRouter capability report

**Status: DOCUMENTED, NOT VALIDATED.** Every verdict below comes from public
OpenRouter documentation read on 2026-10-29 (UTC). No live request was made, no
Management Key was used, and no account was contacted while writing this
document. Nothing here is evidence for release criterion A01, A03, or A04; those
require real-account reconciliation and remain **Pending** in
[traceability.md](../traceability.md).

Produced by Trellis task P2-01 on branch `jondong/p2-01-capability-fixtures`.
Companion documents: [provider-capability-kimi.md](provider-capability-kimi.md)
and the sanitized shape corpus under [fixtures/](fixtures/).

## Why this document exists

[architecture.md](../architecture.md) requires each adapter to report, per
capability, whether it is supported, unsupported, permission-required, or
unverified — together with the authorization category, the source granularity,
the historical range, and the reporting zone. It also warns that capabilities
vary by account and permission, so they must not be hard-coded from the vendor
alone. This report is the pre-integration half of that obligation: it records
what the published contract promises, so the later live reconciliation has a
written baseline to disagree with.

A documented capability is **not** a working capability. The column that matters
for release is the last one.

## Sources

| Surface | Document | Read |
| --- | --- | --- |
| Account Credits | <https://openrouter.ai/docs/api/api-reference/credits/get-remaining-credits> | 2026-10-29; OpenAPI schema re-read 2026-10-30 |
| Current key | <https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key> | 2026-10-29; OpenAPI schema re-read 2026-10-30 |
| Activity | <https://openrouter.ai/docs/api/api-reference/analytics/get-user-activity-grouped-by-endpoint> | 2026-10-29 |
| Analytics query | <https://openrouter.ai/docs/api/api-reference/analytics/query-analytics-data> | 2026-10-29 |
| Analytics meta | <https://openrouter.ai/docs/api/api-reference/analytics/get-available-analytics-metrics-and-dimensions> | 2026-10-29 |
| Management keys | <https://openrouter.ai/docs/guides/overview/auth/management-api-keys.md> | 2026-10-29 |
| Limits | <https://openrouter.ai/docs/api_reference/limits.md> | 2026-10-29 |
| Activity dashboard | <https://openrouter.ai/docs/guides/features/activity.md> | 2026-10-29 |
| Workspace budgets | <https://openrouter.ai/docs/guides/features/workspaces/workspace-budgets.md> | 2026-10-29 |

Field names, example values, error bodies, and the quoted sentences below are
taken from those pages. Where a value is invented for a fixture it is labelled
as invented.

## The four surfaces are not interchangeable

OpenRouter exposes four distinct read surfaces, and
[requirements.md](../requirements.md) treats conflating them as a correctness
failure rather than a display preference.

### 1. Account Credits — `GET /api/v1/credits`

The account-level prepaid balance. Documented as "Get total credits purchased
and used for the authenticated user. **Management key required.**"

Published success shape:

```json
{ "data": { "total_credits": 100.5, "total_usage": 25.75 } }
```

- `total_credits` is **cumulative purchased** credits, not cash spent. Top-up
  payments, transaction fees, usable credits, and consumed credits are four
  different quantities; requirements.md forbids labelling cumulative purchases
  as total cash expenditure.
- `total_usage` is **cumulative consumed** credits across the account's whole
  life, not a selected period.
- Remaining credits is a **derived** value (`total_credits - total_usage`), not a
  field. It can go negative: the P2-02 parser reports a negative remainder as
  `Missing` rather than clamping it to zero, and the published limits guide
  confirms a negative balance can produce errors even for free models.
- The response carries **no timestamp**, **no currency field**, **no period**,
  and **no top-up breakdown**. Consequently `source_as_of` must stay `None` for
  this dataset, and the accounting currency is an assumption from the
  documentation rather than a value the payload vouches for.

This is the only surface P2-02 actually calls. See
[crates/providers/src/openrouter/credits.rs](../../crates/providers/src/openrouter/credits.rs).

### 2. Current key budget — `GET /api/v1/key`

The **per-key** view. Documented as "Get information on the API key associated
with the current authentication session" — a regular inference key can call it,
and it is *not* in the management-key route list.

The limits guide publishes a hand-written type for the response, comments included.
It is an **abbreviated subset** of the real schema; the corrections follow it:

```typescript
type Budget = {
  scope: 'workspace_default_member' | 'workspace_default_key' | 'member'
       | 'api_key' | 'workspace' | 'key_limit';
  guardrail_id: string | null; // Set for guardrail scopes
  budget_id: string | null;    // Set for the workspace scope
  limit_usd: number;
  spend_usd: number;           // Spend counted against the limit in the current window
  remaining_usd: number;       // limit_usd - spend_usd; zero or negative blocks requests
  reset_interval: 'daily' | 'weekly' | 'monthly' | null; // null never resets
  resets_at: string | null;    // ISO 8601 UTC instant the window rolls over, or null
  include_byok: boolean;       // Whether external BYOK spend counts toward this budget
};

type Key = {
  data: {
    label: string;
    limit: number | null;           // Credit limit for the key, or null if unlimited
    limit_reset: string | null;     // Type of limit reset, or null if never resets
    limit_remaining: number | null; // Remaining credits for the key, or null if unlimited
    include_byok_in_limit: boolean;

    budgets: Budget[];               // Every budget enforced for this key,
                                     // in the order enforcement checks them
    effective_budget: Budget | null; // The budget that blocks requests first, or null

    usage: number;         // Credits used (all time)
    usage_daily: number;   // Credits used (current UTC day)
    usage_weekly: number;  // ... (current UTC week, starting Monday)
    usage_monthly: number; // ... (current UTC month)

    byok_usage: number;    // The same four windows for external BYOK usage
    byok_usage_daily: number;
    byok_usage_weekly: number;
    byok_usage_monthly: number;

    is_free_tier: boolean; // Whether the user has paid for credits before

    free_model_daily_requests: {
      used: number;      // Free-model requests recorded so far in the current UTC day
      limit: number;     // Free-model requests allowed per UTC day
      remaining: number; // Free-model requests left in the current UTC day
    };
    // rate_limit: { ... } // A deprecated object in the response, safe to ignore
  };
};
```

Semantics that change how the numbers may be used:

- **`null` means unlimited, not unknown.** `limit: null` and `limit_remaining: null`
  are the documented encoding of a key with no credit cap. An adapter must render
  that as *no key cap*, and requirements.md is explicit that an unlimited key
  budget still does not imply unlimited account funds.
- **`remaining_usd` is derived and may be zero or negative.** The schema states
  `remaining_usd = limit_usd - spend_usd` and that "Zero or negative means this layer
  rejects requests with a 403 until it resets." A negative remainder is a real state,
  not a parse error — the same posture P2-02 already takes for a negative account
  balance — and it is one more source of the ambiguous 403 described under
  [Error and permission semantics](#error-and-permission-semantics).
- **`budgets[]` is ordered by enforcement**, and `effective_budget` names the one
  that blocks first. Enforcement order is guardrail budgets → workspace budgets →
  the key's own limit. Treating `budgets[]` as a set to be summed would
  double-count one allowance expressed at three scopes.
- **`is_free_tier` does not mean "free plan", and the two documents disagree on what
  it does mean.** The limits guide calls it "whether the user has paid for credits
  before"; the OpenAPI schema calls it "Whether this is a free tier API key". Either
  way it is an account-history or tier flag, not a plan name, and requirements.md
  forbids guessing a plan from quota size. The schema adds that
  `free_model_daily_requests.limit` "depends on total credits purchased and is
  independent of `is_free_tier`", so the flag does not even predict the free-model
  allowance. Record the conflict; build on neither reading until reconciliation picks
  one.
- **`rate_limit` is deprecated and self-contradictory.** The OpenAPI schema publishes
  it as an object with three *required* properties — `interval` (string), `requests`
  (integer), and `note` (string) — and marks the whole object `deprecated: true` with
  the description "Legacy rate limit information about a key. Will always return -1."
  That description contradicts the schema's own example values (`1h` / `1000`), which
  the fixtures here reproduce. Parse the object and discard it; never surface it as a
  quota or a rate limit. It is *not* an invented sub-shape, and dropping it would omit
  a required field.
- **BYOK spend is a separate accounting stream.** `byok_usage*` is spend through
  the user's own upstream provider keys and does not draw account credits, while
  `include_byok_in_limit` says whether it counts against the key cap. Adding it to
  credit consumption would mix two different accountabilities.

The **authoritative** shape is the OpenAPI 3.1 document published inline on the
API-reference page (`https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key`,
read 2026-10-30), not the abbreviated type above. Its `data` object lists **24 required
fields**, and they correct an earlier reading of this section:

- `is_management_key`, `is_provisioning_key`, `creator_user_id`, `organization_id`,
  `workspace_id`, `allowed_data_regions`, and `rate_limit` are all published properties,
  and every one of them is **required**. `expires_at` is published and nullable but is
  *not* in the required list. So these fields are documented parts of the wire shape —
  the limits guide's `Key` type simply omits them. What remains unmeasured is their
  *values*: no live response has been observed, so no code may depend on a particular
  field being populated until reconciliation confirms it.
- `is_provisioning_key` is marked `deprecated: true` and described as "Whether this is a
  management key" — a second, dying name for the same idea. Prefer `is_management_key`,
  which is the cheapest honest way to detect the presented key's **permission category**
  without probing the balance (architecture.md forbids using inference calls to test
  authentication).
- `organization_id` is "The ID of the organization that owns this API key, or null when
  an individual account owns it"; `workspace_id` is "The ID of the workspace this API key
  spends in, or null when no active workspace resolved for it, for example because the
  key's workspace was deleted." These are the strongest identity candidates on the whole
  surface, and they narrow — though they do not close — the `provider_scope_id` question
  in [Capability matrix](#capability-matrix) row 22.
- `allowed_data_regions` is an enum of `global`, `europe`, and `us`, "Empty when no
  region is permitted", and reflects *region policy only* — other restrictions, such as
  management keys being blocked from inference, still apply. It is not a capability list.
- This route documents only **401** and **500** responses. There is no 403, unlike
  `/credits`, so `/key` cannot distinguish "insufficient permission" from
  "unauthenticated" by status alone. `error.metadata` is a nullable free-form object, and
  error envelopes may additionally carry `openrouter_metadata` and `user_id`.

`label` is already masked by the API, e.g. `"sk-or-v1-au7...890"`, so it is a truncated
key-shaped string rather than a secret — fixtures in this repository nonetheless avoid
that prefix entirely so secret scanners stay clean.

Two rules from requirements.md follow from the whole surface:

1. **A key budget is not an account balance.** `limit_remaining` describes one
   key's allowance. Summing it across keys, or presenting it as the account
   balance, double-counts a shared pool.
2. **With only a regular key the account balance is simply unavailable.** The
   honest state is "supported key information shown, account-balance permission
   missing" — which requirements.md says does *not* satisfy account-balance
   acceptance.

### 3. Activity — `GET /api/v1/activity`

Daily usage grouped by model endpoint. Documented verbatim as:

> Returns user activity data grouped by endpoint for the last 30 (completed) UTC
> days. […] Activity recorded before workspace resolution existed is permanently
> attributed to the account default workspace (no backfill is possible).
> **Management key required.**

Published row shape:

```json
{
  "data": [
    {
      "byok_usage_inference": 0.012,
      "cached_tokens": 10,
      "completion_tokens": 125,
      "date": "2025-08-24",
      "endpoint_id": "550e8400-e29b-41d4-a716-446655440000",
      "model": "openai/gpt-4.1",
      "model_permaslug": "openai/gpt-4.1-2025-04-14",
      "prompt_tokens": 50,
      "provider_name": "OpenAI",
      "reasoning_tokens": 25,
      "requests": 5,
      "usage": 0.015
    }
  ]
}
```

Query parameters: `date` (a single UTC date within the last 30 days,
`YYYY-MM-DD`), `api_key_hash` (SHA-256 hex, as returned by the keys API),
`user_id` (org member, organization accounts only), `workspace_id` (UUID), and
`group_by=workspace` (the only allowed value; splits rows per workspace and adds
`workspace_id` to every item — omitting it aggregates across workspaces and
preserves the historical shape).

Hard properties that constrain any history design:

- **30 completed UTC days is a retention bound, not a default.** Today is never
  included, and days older than the window are gone.
- **No cursor, no limit parameter, and no truncation flag are documented.**
  There is nothing to paginate and nothing that attests completeness beyond the
  endpoint's own "last 30 (completed) UTC days" statement.
- `date` is a **calendar date string**, not an instant. A row therefore denotes a
  whole UTC day and must be rendered as UTC; requirements.md forbids splitting
  it proportionally across local days.
- Because the window contains only completed days, **Activity cannot establish a
  total for today**. The Today view must say the data is unavailable rather than
  showing zero.

### 4. Analytics — `POST /api/v1/analytics/query` and `GET /api/v1/analytics/meta`

The general-purpose aggregate surface, and the documented programmatic
equivalent of the dashboard's Explore tab. **Management key required** for both.

Request body: `metrics` (string[], required, minimum length 1), `dimensions`
(string[], maximum length 2 — "Use the /meta endpoint for available
dimensions"), `filters` (object[], maximum length 20), `granularity` (e.g.
`"day"`), `time_range` (e.g. `{"start":"2025-01-01T00:00:00Z","end":"2025-01-08T00:00:00Z"}`),
`group_limit`, `limit` (defaults to 1000), `order_by`, plus
`classifier_dimensions` / `classifier_filters` for custom classifier tags.

Published success shape:

```json
{
  "data": {
    "data": [{ "date__day": "2025-01-01T00:00:00.000Z", "request_count": 1500 }],
    "metadata": { "query_time_ms": 42, "row_count": 1, "truncated": false }
  }
}
```

- `data.metadata.truncated` is the **only** completeness attestation OpenRouter
  publishes. It maps directly onto `HistoryPage.complete` in
  [crates/core/src/provider.rs](../../crates/core/src/provider.rs): a truncated
  response must never publish a partition as complete.
- **No cursor or offset pagination is documented.** Truncation is terminal, not
  resumable — the remedy is a narrower `time_range`, fewer dimensions, or a
  higher `limit`, never a continuation token.
- Time-bucket column names are granularity-suffixed (`date__day`), so the row
  shape depends on the requested granularity and cannot be a fixed struct.
- `group_limit` is auto-computed when omitted on time-series queries; an explicit
  value **may truncate time buckets**. The dashboard's Explore tab likewise
  collapses series beyond its limit into an "Other" row — a silent hazard for
  exact totals.
- The documented error set includes **408** `"Operation timed out. Please try
  again later."`, a server-side timeout that is distinct from a transport
  timeout and must not be mapped to `ProviderError::Timeout` without a decision.
- `/analytics/meta` returns the available vocabulary:
  `{ dimensions:[{name,display_label}], granularities:[{name,display_label}], metrics:[{name,display_label,display_format,is_rate}], operators:[{name,value_type}] }`.
  Because that vocabulary is account- and time-dependent, it must be **discovered
  at runtime**, exactly as architecture.md instructs. Metrics flagged
  `is_rate: true` are ratios and must never be summed across buckets.

Analytics is also the only documented path to a range longer than 30 days, which
matters because requirements.md allows a custom range of up to 366 calendar days.

## Capability matrix

Verdicts use the architecture.md vocabulary. **Unverified** means the contract
is documented but no live call has confirmed it for any account.

| # | Capability | Surface | Authorization | Verdict in code today | Granularity | Historical range | Zone |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Remaining account credits | `/credits` | Management key | Supported, **Unverified** | Account, cumulative | Lifetime only | Not stated by payload |
| 2 | Cumulative purchased credits | `/credits` `total_credits` | Management key | Supported, **Unverified** | Account, cumulative | Lifetime only | Not stated |
| 3 | Cumulative consumed credits | `/credits` `total_usage` | Management key | Supported, **Unverified** | Account, cumulative | Lifetime only | Not stated |
| 4 | Key permission category | `/key` `is_management_key` | Regular key | **Unsupported** (not implemented) | Key | n/a | n/a |
| 5 | Key credit budget + remainder | `/key` `limit`, `limit_remaining`, `limit_reset` | Regular key | **Unsupported**, reported explicitly | Key | Current only | Not stated |
| 6 | Key spend per window | `/key` `usage_daily`/`weekly`/`monthly` | Regular key | **Unsupported** | Key | Current windows only | UTC per the limits guide |
| 7 | Guardrail / workspace budgets | `/key` `budgets[]`, `effective_budget` | Regular key | **Unsupported** | Key/workspace/guardrail | Current only | UTC |
| 8 | BYOK spend (not account credits) | `/key` `byok_usage*` | Regular key | **Unsupported** | Key | Current windows only | UTC |
| 9 | Daily input/output tokens | `/activity` `prompt_tokens`, `completion_tokens` | Management key | **Unsupported** | Day × model × endpoint | Last 30 **completed** UTC days | UTC |
| 10 | Daily cached tokens | `/activity` `cached_tokens` | Management key | **Unsupported** | Day × model × endpoint | Last 30 completed UTC days | UTC |
| 11 | Daily reasoning tokens | `/activity` `reasoning_tokens` | Management key | **Unsupported** | Day × model × endpoint | Last 30 completed UTC days | UTC |
| 12 | Daily credit consumption | `/activity` `usage`, `byok_usage_inference` | Management key | **Unsupported** | Day × model × endpoint | Last 30 completed UTC days | UTC |
| 13 | Daily request count | `/activity` `requests` | Management key | **Unsupported** | Day × model × endpoint | Last 30 completed UTC days | UTC |
| 14 | Arbitrary-range aggregates (≤366 days) | `/analytics/query` | Management key | **Unsupported** | Requested granularity × ≤2 dimensions | Requested `time_range` | UTC bucket labels |
| 15 | Truncation attestation | `/analytics/query` `metadata.truncated` | Management key | **Unsupported** | Per query | n/a | n/a |
| 16 | Metric/dimension vocabulary | `/analytics/meta` | Management key | **Unsupported** | n/a | n/a | n/a |
| 17 | Workspace attribution | `/activity?group_by=workspace` | Management key | **Unsupported** | Day × model × endpoint × workspace | Last 30 completed UTC days | UTC |
| 18 | Today's live total | none | n/a | **Unsupported by the source** | n/a | n/a | n/a |
| 19 | Top-up payments and fees | none | n/a | **Unsupported by the source** | n/a | n/a | n/a |
| 20 | Server-side as-of timestamp for credits | none | n/a | **Unsupported by the source** | n/a | n/a | n/a |
| 21 | Account currency from the payload | none | n/a | **Unsupported by the source** | n/a | n/a | n/a |
| 22 | Account identity (`provider_scope_id`) | `/key` `organization_id`, `workspace_id` (both required); `/activity` `workspace_id` | Regular / management key | **Unsupported**, authority unmeasured | Organization / workspace | n/a | n/a |

### Required metrics that no documented surface can satisfy

These are called out separately because they are required by
[requirements.md](../requirements.md) and are **not** implementation gaps that
more code would close:

- **Today's consumption as a settled total.** Activity covers only *completed*
  UTC days. Analytics accepts an arbitrary `time_range` and may therefore return
  a partial current day, but no documentation states whether the current UTC day
  is included, provisional, or absent. Until that is measured, the Today view
  must report unavailable data — never zero, and never a partial value presented
  as final.
- **Top-up payments, transaction fees, and the distinction between cash spent and
  credits granted.** `/credits` returns two lifetime counters only. There is no
  documented ledger of purchases, so "cumulative purchased credits" must never be
  labelled as cash expenditure.
- **A source-supplied as-of instant or currency for the balance.** The credits
  payload has neither. `source_as_of` stays `None`, and the currency is a
  documented assumption that live reconciliation must confirm.
- **An account identity carried by the credits payload itself.** `/credits` returns two
  counters and names no account, so attributing a balance to a
  `provider_scope_id` always needs a second call. `/key` does publish the identity
  fields — `organization_id` and `workspace_id` are both **required**, with documented
  null branches ("null when an individual account owns it" and "null when no active
  workspace resolved for it") — but which one is authoritative for deduplication is
  **unmeasured**, and `creator_user_id` identifies a *member* rather than an account.
  Since architecture.md keys deduplication on `provider_scope_id`, and management keys
  are account-scoped and unscoped across workspaces, guessing risks merging two
  accounts or splitting one.

## Error and permission semantics

| Trigger | Published response | Correct mapping |
| --- | --- | --- |
| No `Authorization` header | 401 `{"error":{"code":401,"message":"Missing Authentication header"}}` | `ProviderError::PermissionRequired`, before any network call |
| Regular key on a management route | 403 `{"error":{"code":403,"message":"Only management keys can perform this operation"}}` | Dataset-scoped permission failure; other datasets keep their rows |
| Expired management key | 401 with message `API key expired` | Permission failure, and a rotation prompt — expiry is fixed at creation and cannot be extended |
| Bad parameters | 400 `{"error":{"code":400,"message":"Invalid request parameters"}}` | `ProviderError::InvalidResponse` |
| Rate limited | 429, plus `X-RateLimit-*` and possibly `Retry-After` | `ProviderError::RateLimited { retry_after }` |
| Server-side analytics timeout | 408 `"Operation timed out. Please try again later."` | Distinct from transport timeout; decision pending |
| Resource missing | 404 `{"error":{"code":404,"message":"Resource not found"}}` | `ProviderError::InvalidResponse` |
| Upstream failure | 500 `{"error":{"code":500,"message":"Internal Server Error"}}` | Retry per service policy, not in the parser |

Three traps worth recording:

**A 403 is ambiguous.** The same status covers "management scope missing" and
"workspace budget exceeded" (the latter names the broadest exceeded interval).
The adapter must read the error body; it may not infer a permission verdict from
the status line alone.

**Credit and in-flight limits surface as a 402 with metadata, not only a 429.**
The limits guide publishes the envelope exactly:

```json
{
  "error": {
    "code": 402,
    "message": "This request would exceed your available credits given your current in-flight requests. Retry after in-flight requests settle, or add credits.",
    "metadata": {
      "reason": "in_flight_budget_exhausted",
      "limit_source": "openrouter_in_flight_budget",
      "remedy_hint": "Retry after your in-flight requests settle (see the Retry-After header). Adding credits at https://openrouter.ai/settings/credits raises your in-flight budget, up to a capped ceiling."
    }
  }
}
```

All three discriminating fields live under `error.metadata`. `limit_source` is one
of `openrouter_in_flight_budget` (transient; the response includes `Retry-After`),
`openrouter_key_limit` (the key's own credit cap is exhausted), or
`openrouter_credits` (the balance cannot cover the request, or the single request
is too expensive); `reason` is `in_flight_budget_exhausted` or
`weight_exceeds_budget`. `remedy_hint` is prose for people reading logs — **branch
on `limit_source`, never on the hint text**. These are inference-path errors, so a
monitoring client will not normally see them; they are recorded here because
`openrouter_credits` versus `openrouter_key_limit` is exactly the
account-versus-key distinction A04 cares about, and because a negative account
balance can produce these errors "including for free models".
**Management keys are dangerous by construction.** The same credential that
unlocks `/credits`, `/activity`, and `/analytics/*` also unlocks
`POST|DELETE|PATCH /api/v1/keys`, `/api/v1/byok`, guardrails, and organization
settings. The documentation states a leaked management key "could create, edit
and delete the API keys in your account". The read-only allow-list therefore has
to be enforced in code, not left to operator discipline. Management keys also
**cannot** call completion endpoints, which is a useful property: a monitoring
credential cannot be spent on inference.

## Reporting zone

The limits guide states it directly:

> Spend windows are UTC: a daily budget resets at 00:00 UTC, a weekly one on
> Monday 00:00 UTC, a monthly one on the first of the month 00:00 UTC. A
> lifetime workspace budget never resets.

Workspace budgets confirm the same four intervals (daily / weekly / monthly /
lifetime) with the same UTC boundaries. This matches requirements.md: "This week"
begins on Monday, and UTC daily buckets are displayed as UTC rather than split
into local days. Activity's `date` field and Analytics' `date__day` bucket labels
are both UTC.

## Fixture inventory

Two sets exist. The adapter-local set is parsed by tests; the corpus under
`docs/validation/fixtures/` is documentation-grade shape evidence and is not
parsed by the adapter.

### Landed in P2-02 — `crates/providers/src/openrouter/fixtures/`

| File | Contents | Consumed by |
| --- | --- | --- |
| `credits_ok.json` | `{"data":{"total_credits":"120.50","total_usage":"37.25"}}` | `parses_string_amounts_from_fixture` (credits.rs:245), `credits_success_maps_money_metrics` (mod.rs:338), `key_budget_is_reported_unsupported_not_conflated` (mod.rs:357) |
| `credits_number_amounts.json` | Bare numbers `120.055` / `37.257` | `parses_bare_number_amounts_exactly` (credits.rs:263) |
| `credits_overdrawn.json` | `10.00` / `12.50` (negative remainder) | `negative_remaining_is_reported_missing_not_clamped` (credits.rs:276) |
| `credits_precision_violation.json` | `"1.1234567890123456789"` / `"0"` | `precision_violation_is_invalid` (credits.rs:300) |
| `credits_missing_field.json` | Only `total_usage` | `missing_amount_field_is_invalid` (credits.rs:291) |
| `credits_forbidden.json` | 403 envelope | `forbidden_maps_to_dataset_permission_failure` (mod.rs:384) |
| `credits_rate_limited.json` | 429 envelope | `rate_limit_carries_cooldown_from_retry_after_seconds` (mod.rs:424) |
| `credits_malformed.txt` | Truncated JSON | `malformed_body_is_invalid` (credits.rs:309), `malformed_success_body_fails_credits_dataset_only` (mod.rs:509) |
| `activity_shape_only.json` | Self-labelled shape stub, **not parsed by any code path** | nothing — named only in a `history_page` doc comment (mod.rs:241) |

Citation convention: a line number is the `fn` declaration line of that test in
`crates/providers/src/openrouter/`, as of commit `1f689e1`. These drift when the crate
is edited, so the test **name** is the durable identifier and the line is a
convenience.

All values in that directory are invented. No file contains a credential, an
authorization header, or a real account identifier.

### ⚠ `activity_shape_only.json` does not match the published Activity schema

The P2-02 stub uses `id`, `day`, `api_key`, `input_tokens`, `output_tokens`,
`cache_read_tokens`, `cache_write_tokens`, and `cost`. The published
`/api/v1/activity` row uses `date`, `prompt_tokens`, `completion_tokens`,
`cached_tokens`, `reasoning_tokens`, `usage`, `requests`, `model`,
`model_permaslug`, `endpoint_id`, `provider_name`, and `byok_usage_inference` —
and has **no** `id`, no `api_key` field (the filter is `api_key_hash`, a SHA-256
digest in the query string, never in the row), no cache-write counter, and no
`cost` field.

The stub already declares its own wire shape "UNVALIDATED", so this is a
correction rather than a contradiction. It was left in place because P2-01 must
not restructure the providers crate while P2-03 works in it concurrently. The
corrected shapes are published at
[fixtures/openrouter/](fixtures/openrouter/) and **whoever implements Activity
parsing must supersede the stub rather than copy it.**

### Added by P2-01 — `docs/validation/fixtures/openrouter/`

See [fixtures/README.md](fixtures/README.md) for the sanitization rules and the
smoke check.

| File | Documents |
| --- | --- |
| `activity_ok.json` | Published `/activity` row shape with invented values |
| `activity_grouped_by_workspace.json` | `group_by=workspace` row shape including `workspace_id` |
| `analytics_query_request.json` | A conforming `/analytics/query` request body |
| `analytics_query_ok.json` | Success envelope with `metadata.truncated: false` |
| `analytics_query_truncated.json` | `metadata.truncated: true` — must not publish a complete partition |
| `analytics_query_server_timeout.json` | The documented 408 envelope |
| `analytics_meta.json` | `/analytics/meta` vocabulary shape |
| `current_key_regular.json` | `/key` for a regular inference key with a cap |
| `current_key_management.json` | `/key` for a management key (`is_management_key: true`) |
| `current_key_unlimited.json` | A key with no cap — an unlimited key budget does not imply unlimited account funds |
| `credits_insufficient_scope.json` | 403 "Only management keys can perform this operation" |
| `credits_expired_key.json` | 401 `API key expired` |
| `limit_source_credits.json` | The 402 `error.metadata.limit_source` shape; the documented `reason`/`limit_source` values are used verbatim, the `message`/`remedy_hint` prose is synthetic |

## Open questions for live reconciliation

Recorded here so the credential-holder can answer them in one session. None may
be assumed.

1. Does `/credits` return bare numbers or strings in practice? The published
   example uses bare numbers; the P2-02 fixtures use strings; the parser accepts
   both. Which one a real account emits decides whether one branch is dead code.
2. Is the credits currency USD, and is it stated anywhere in the live response?
3. Does `/analytics/query` include the current, unfinished UTC day, and if so is
   the row marked provisional in any way?
4. What is the real maximum `time_range` width, and what error comes back beyond
   it? Requirements allow a 366-day custom range; nothing documents whether
   Analytics serves it in one call.
5. Which metric names does `/analytics/meta` actually return for a prepaid
   personal account, and which are `is_rate: true`?
6. Which `/key` identity field is authoritative for `provider_scope_id`? The schema
   publishes `organization_id` (null for an individual account) and `workspace_id`
   (null when no active workspace resolved), both required — but management keys are
   account-scoped and unscoped across workspaces. What does a personal account
   actually return, and does either field survive credential rotation?
7. Under what conditions does `truncated` come back `true`, and does the server
   ever raise `limit` on its own as the documentation suggests?
8. How does an overdrawn balance present itself — a negative
   `total_credits - total_usage`, or something else?
9. Does `/activity` return rows for days with zero usage, or omit them? An omitted
   day and a zero day are different facts, and requirements.md forbids treating a
   gap as zero without confirmation.

## Honest status

| Criterion | Status | Why |
| --- | --- | --- |
| A01 provider adapters and capability reports | **Pending** | This report is documentation. A01 requires a real OpenRouter prepaid account reconciled against the console. |
| A04 metric scope and permission state | **Pending** | The key-budget dataset is reported `Unsupported` in code, which is honest but not a substitute for a measured regular-key-vs-management-key comparison. |
| A05 metric transport and UI states | **Pending** | Zero, null, unsupported, stale, and failed states are modelled and unit-tested against fixtures, not against live responses. |
| G02 live credentials | **Open blocker** | No Management Key is available in this worktree, and none may be committed. |

Per the phase-two-plan exit gate: live-key absence is an open credential blocker,
not a documentation pass.
