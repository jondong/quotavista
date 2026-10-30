# Kimi capability report

**Status: DOCUMENTED, NOT VALIDATED — retrieval path unresolved.** Every verdict
below comes from public Kimi documentation read on 2026-10-29 (UTC). No account
was contacted, no session was used, and no credential was read while writing this
document. Nothing here is evidence for release criterion A01, A02, or A03; those
require comparison against a real member's official subscription page and remain
**Pending** in [traceability.md](../traceability.md).

Produced by Trellis task P2-01 on branch `jondong/p2-01-capability-fixtures`.
Companion documents:
[provider-capability-openrouter.md](provider-capability-openrouter.md) and the
sanitized shape corpus under [fixtures/](fixtures/).

## Sources

| Subject | Document | Read |
| --- | --- | --- |
| Membership benefits and tiers | <https://www.kimi.com/help/membership/membership-overview> (English: <https://www.kimi.com/en/help/membership/membership-overview>) | 2026-10-29 |
| Quota update and usage rules | <https://www.kimi.com/help/membership/membership-update-rules> (English: <https://www.kimi.com/en/help/membership/membership-update-rules>) | 2026-10-29 |
| Extra Usage (加油包) | <https://www.kimi.com/help/membership/membership-extra-usage> | 2026-10-29 |
| Benefit arrival and where to look | <https://www.kimi.com/help/membership/membership-account-query> | 2026-10-29 |
| Kimi Code membership benefits | <https://www.kimi.com/code/docs/kimi-code/membership.html> (English: `/code/docs/en/kimi-code/membership.html`) | 2026-10-29 |
| Kimi Code CLI local server API | <https://www.kimi.com/code/docs/kimi-code-cli/reference/server-api.html> | 2026-10-29 |
| Kimi API Open Platform overview | <https://www.kimi.com/help/kimi-api/api-overview> | 2026-10-29 |
| Platform balance and usage lookup | <https://www.kimi.com/help/kimi-api/api-balance-and-usage> | 2026-10-29 |
| Platform rate limits | <https://www.kimi.com/help/kimi-api/api-rate-limits> | 2026-10-29 |

Quoted Chinese is reproduced with an English translation. Where the English help
page carries the same fact, the official English wording is used: *credit pool*,
*rolling 5-hour usage limit*, *weekly limit*, *monthly shared credit pool*.

## Three products, three wallets — only one of them is in scope

This is the single most important fact in the report, and the one
[requirements.md](../requirements.md) treats as a correctness boundary.

| Product | Where | Billing | In scope for QuotaVista's Kimi connection? |
| --- | --- | --- | --- |
| **Kimi membership** (会员) | kimi.com web, App, Kimi Work, Kimi Claw, Kimi Code | Monthly or annual **subscription**; quota refreshes monthly | **Yes — this is the target** |
| **Kimi API Open Platform** (开放平台) | platform.kimi.com, `api.moonshot.cn` | **Pay-as-you-go only, no subscription plans** | **No** |
| **Kimi Business** | team offering | Annual subscription, minimum 5 seats | No (out of scope for P0/P1) |

The Open Platform overview states it directly:

> Kimi API 开放平台是按量计费模式、无订阅制方案，与 Kimi 会员、Kimi Code 等产品不同，请注意区分。
>
> The Kimi API Open Platform is a pay-as-you-go billing model with no subscription
> plans, which differs from Kimi membership, Kimi Code and other products — please
> distinguish them.

And, on the relationship between the two APIs:

> 注意：Kimi 会员目前包含 Kimi Code 相关权益，Kimi Code API 与本平台提供的 API 服务相互独立。
>
> Note: Kimi membership currently includes Kimi Code related benefits; the Kimi
> Code API and the API services provided by this platform are mutually
> independent.

Consequences that must survive into the adapter:

- **The platform balance endpoint cannot substitute for membership quota.**
  `GET https://api.moonshot.cn/v1/users/me/balance` reports the Open Platform
  wallet (documented to include a `voucher_balance` field; the public page does
  not document the full schema). That wallet is a different product with a
  different billing model. Displaying it as "Kimi quota" would be the exact
  substitution requirements.md forbids.
- **Independent Kimi Code limits cannot substitute either.** The rolling 5-hour
  window and the legacy weekly limit are *rate* limits on one feature. Retrieving
  only those does not satisfy the integration.
- **Extra Usage is a third balance.** It is a CNY-denominated prepaid wallet, not
  a quota percentage, and requirements.md defers it to P1 while insisting it stay
  visibly separate from included subscription quota.

## The shared membership pool

### What shares it

Per the quota rules page, one pool covers the Agent series (website generation,
document processing, PPT, sheets, deep research), Kimi Code, Kimi Work, Kimi
Claw, and other benefits such as image generation. The Kimi Code documentation
adds that requests from the **CLI, VS Code, desktop, and third-party tools all
count against the same membership quota**, and that **all logged-in devices and
API keys share one quota**.

This is the evidence for release criterion A02: web, mobile, and Code are one
pool and must be counted **once**. A design that created one connection per
surface, or per Code API key, would multiply a single balance.

Deduction is by **actual token consumption** of each task, input and output both.
The same task costs differently depending on model, context length, tool calls,
and plan. The documentation is explicit that **the usage-detail records are
authoritative** (以使用明细为准).

### Refresh timing

| Rule | Documented behaviour |
| --- | --- |
| Cadence | **Monthly for both monthly and annual subscriptions** |
| Anchor | The **subscription cycle**, not the calendar month |
| Worked example | "if you subscribe at 3:00 PM on December 1, your next monthly refresh is at 3:00 PM on January 1. Check your subscription page for the exact time" |
| Rollover | None — unused quota is cleared at the end of the cycle and the full allowance is reissued |
| Cash conversion | Not permitted (不结转、不折现) |
| Expiry | Unused benefits within the subscription term are cleared automatically when it expires |

**The refresh boundary is not a UTC month boundary.** This is a hard constraint
on the statistics design, not a display detail. A member who subscribed at 15:00
local time has a period that starts mid-day and mid-month, in a zone the
documentation does not specify. requirements.md already anticipates the case: a
provider that cannot support the requested day boundaries must have its native
series shown separately, with incompatible values excluded from any common total.
Kimi's monthly percentage therefore **cannot** be summed with, or bucketed into,
a UTC calendar month alongside OpenRouter data.

Payment frequency, membership validity, and quota refresh are three different
clocks and must not be collapsed. In particular the exact refresh time must be
**read from the subscription page**, never inferred from a payment or renewal
date.

### Kimi Code limits, new versus legacy plans

| Rule | New plans (Plus and higher) | Legacy plans (Andante and higher) |
| --- | --- | --- |
| Rolling 5-hour usage limit | Yes | Yes |
| Weekly limit | **No** | Yes — resets every 7 days from the subscription start time |
| Monthly shared credit pool | Applies | Applies |
| Kimi Code access | Plus and above | Andante and above |

The English page cautions: "A rolling 5-hour limit caps usage within that window;
it does not mean you can use Kimi Code for only five hours per week. **Go does
not include Kimi Code.**"

When the monthly pool is exhausted, **Kimi Code quota is frozen** until the
monthly reset or a subscription upgrade — so the Code limits and the pool
interact, but the pool is the binding constraint. Legacy plans are unaffected by
the new tier launch: names, quota rules, and auto-renewal all persist, and
migration is not forced. An account's plan generation therefore cannot be
inferred from the date; it has to be read.

## What can be retrieved, and how

### Path A — the official subscription page (authoritative, human-visible)

Reachable at `https://www.kimi.com/membership/subscription` with a `?tab=quota`
variant, and in-app at 我的 → 会员计划 → 订阅. It shows:

1. the current quota balance **as a percentage**,
2. the **next refresh time**,
3. the **most recent 1000 usage-detail records**, each with usage time, feature,
   and consumption ratio/percentage.

It carries an explicit delay warning:

> 使用明细数据可能存在短暂延迟，请以当前额度显示为准
>
> Usage-detail data may be briefly delayed; treat the current quota display as
> authoritative.

That sentence is the officially documented reporting delay A03 must record, and
it establishes an ordering: the percentage is more current than the itemized
history. **These are login-gated UI screens. No public JSON API for membership
quota is documented anywhere in the help centre.**

### Path B — the Kimi Code CLI local server API (documented, experimental, third-party-owned)

The Kimi Code CLI/Desktop process serves a REST + WebSocket API, default
`http://127.0.0.1:58627`. One endpoint is directly relevant:

**`GET /api/v1/oauth/usage?provider=managed:kimi-code`** — "托管账号的套餐额度与加油包，实时取自账号服务"
(the managed account's plan quota and Extra Usage, fetched live from the account
service). An upstream failure does **not** fail the envelope; it is returned
in-band as `kind: "error"`.

Success `data` is `{ kind: "ok", quota }` or `{ kind: "error", message, status? }`,
where `status` is the upstream HTTP status when present. In the `ok` shape:

```text
quota.usages     — entries keyed by window: limit5h, limit7d, monthTotal, monthCode
                   each { usedRatio: 0–1 float, resetAt?: RFC3339 }
quota.extraUsage — nullable pay-as-you-go wallet:
                   { balanceCents, totalCents, monthlyChargeLimitEnabled,
                     monthlyChargeLimitCents, monthlyUsedCents, currency }
```

The documentation states the client renders "按实际下发的条目" — according to the
entries actually delivered — so **the window set is not fixed** and individual
windows may be absent.

Also present: `GET /api/v1/oauth/userinfo` (account profile) and
`GET /api/v1/oauth/region` (local region resolution).

Transport facts: the default binding is **plain HTTP on loopback**; every `/api/*`
path requires a bearer token except `GET /api/v1/healthz`; auth failure is
HTTP 401 with envelope `code` 40101; on non-loopback bindings 10 auth failures
from one origin within 60 seconds trigger a 60-second ban with HTTP 429
(`code` 42901). All JSON responses use the envelope
`{ code, msg, data, request_id }` where **HTTP status is almost always 200 and
`code` carries the business result** — a parser that branches on HTTP status will
misread every error. The whole API is flagged experimental: "不保证接口稳定性，
端点、字段与事件类型可能随任何版本更改" (stability is not guaranteed; endpoints,
fields, and event types may change in any version), and integrators are told to
defer to the `/openapi.json` and `/asyncapi.json` served by the running version.

**This path is a candidate, not an approved design.** Three blockers, in order of
severity:

1. **Authorization.** The token belongs to another tool's local runtime.
   requirements.md is explicit: if access needs a login session, the user must
   authorize a clear connection flow, and QuotaVista must **not** scan or import
   browser cookies or read other tools' credentials by default. Reaching this
   endpoint therefore requires an explicit, user-initiated authorization step in
   which the user supplies the address and token — and even then it depends on a
   third-party process being installed and running.
2. **Stability.** The API is documented as experimental and version-volatile. Any
   integration must re-validate against the served `/openapi.json` at runtime
   rather than trusting a checked-in shape.
3. **Correctness of interpretation.** `monthTotal` is the shared membership pool.
   `limit5h`, `limit7d`, and `monthCode` are the independent Kimi Code limits
   that must **not** substitute for it. An adapter that took the first `usages`
   entry, or merged them, would report a Code rate limit as the membership quota
   — precisely the failure mode requirements.md names.

Until a real member's subscription page has been compared against a real
`oauth/usage` response, Path B stays an open integration question. That
comparison **is** criterion A03.

### PII hazard on Path B

`GET /api/v1/oauth/userinfo` always returns `userId`, `nickname`, `status`,
`region`, `userLevel`, `userLevelName`, `domain`, `domainName`, and **may
additionally return `email`, `phone` (`{ countryCode, number }`), `avatar`,
`username`, `bio`, `globalId`, `createdTime`, `lastLoginTime`**.

QuotaVista needs only `userId` (as `provider_scope_id`), `userLevel` /
`userLevelName` (plan identity), and `region`. Email, phone number, avatar, and
nickname are identifying account data and must **never** be persisted, logged,
echoed into an error, or committed — per requirements.md, the phase-two-plan
credential rules, and criterion A10. A fixture for this endpoint therefore has to
be shape-only with invented values, and the real response must be reduced at
capture time, not after storage.

### Region and currency

`GET /api/v1/oauth/region` resolves the client's Kimi region **locally without
network probing**, in priority order: environment variable or configured OAuth
host → configured OAuth key → the install-channel marker in `<home>/region` →
inference from device timezone and locale. Results cache to `<home>/region.json`
and invalidate on login-state change; raw timezone and locale values are never
uploaded. When signals are missing or conflict it falls back to `mainland-cn` for
the process without persisting that choice, and `KIMI_CODE_REGION_MARKER=off`
disables both the marker and local inference.

Multiple regions therefore exist, and the documented plan lineup is
region-scoped: the English overview opens "**The current China plans are Go,
Plus, Pro, Max, and Ultra**" and captions its price table "China membership
pricing with monthly billing (Chinese interface), captured on October 19, 2026."
Currency for that region is CNY. Plan names, tier count, and prices must be read
from the account and its region — never assumed from this document.

A related inconsistency worth recording: the Open Platform comparison table says
Kimi 提供四档会员订阅套餐 ("four membership tiers") while the membership overview
lists **five** new tiers. The public documentation disagrees with itself, which is
one more reason plan identity must come from `userLevel` / `userLevelName` and
must be reported as *unknown* when it cannot be identified. requirements.md also
forbids inferring a plan name from quota size.

## Extra Usage (加油包) — P1, but its boundaries matter now

| Property | Documented behaviour |
| --- | --- |
| Eligibility | **Subscription members only** |
| Sharing | One balance across web, browser extension, desktop, mobile, and Kimi Code — the 我的额度 top-up and the Kimi Code console balance are the same wallet |
| Unit | **CNY balance**, charged by actual usage; billing depends on input/output volume and **cache hit rate** |
| Price level | "加油包价格近似于 Kimi 开放平台 的官方 API 价格" — approximates the official Open Platform API price |
| Deduction order | Time-limited subscription quota **first**; Extra Usage **last**, as the fallback |
| Expiry | Never expires, stackable, carries to the next month; after the subscription lapses it can still be spent but not topped up |
| Monthly cap | Optional 每月消费上限, default off; must exceed the 25 CNY single minimum |
| Top-up limits | Minimum 25 CNY per top-up, maximum 10 per day and 3,000 CNY cumulative per day, 10,000 CNY balance cap |
| Enterprise | Not supported; the balance is personal-account-only |
| Toggle management | **Web only** — desktop and mobile apps cannot manage it |
| Query paths | Web/App 设置 → 订阅和发票 → 我的额度; Kimi Code console after enabling; Kimi Code CLI `/usage` |

Two design consequences:

- Because Extra Usage is deducted **last** and does not expire, a drop in the
  membership percentage and a drop in the Extra Usage balance are different
  events on different clocks. Counting them together would double-count or mask
  consumption.
- With Extra Usage enabled, hitting any cap switches seamlessly to the balance:
  the task does not error and **membership quota refresh is unaffected**. A period
  in which the user was silently spending Extra Usage therefore looks like low
  membership consumption. Any "no consumption" claim needs the Extra Usage state
  as context.

## Capability matrix

| # | Capability | Documented source | Verdict | Granularity | Historical range | Zone |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Remaining shared quota as a percentage | Subscription page; `oauth/usage` `monthTotal.usedRatio` | **Unverified** (no authorized path) | Account, current period | Current only | Refresh anchor is the subscription instant, zone unstated |
| 2 | Next refresh time | Subscription page; `oauth/usage` `monthTotal.resetAt` | **Unverified** | Account, current period | n/a | RFC3339 (zone-aware) |
| 3 | Plan identity | `oauth/userinfo` `userLevel` / `userLevelName` | **Unverified** | Account | n/a | Region-dependent |
| 4 | Account/scope identity | `oauth/userinfo` `userId` | **Unverified**, PII-adjacent | Account | n/a | n/a |
| 5 | Region | `oauth/region` | **Unverified** | Client install | n/a | Local inference, `mainland-cn` fallback |
| 6 | Consumed quota in a selected range | Subscription page usage detail | **Partial at best** | Per record: time, feature, consumption ratio | **Most recent 1000 records**, no date-range query documented | Unstated |
| 7 | Kimi Code rolling 5-hour limit state | `oauth/usage` `limit5h` | **Unsupported for our purpose** — must not substitute | Feature | Current window only | `resetAt` RFC3339 |
| 8 | Kimi Code weekly limit (legacy plans) | `oauth/usage` `limit7d` | **Unsupported for our purpose** — must not substitute | Feature | Current window only | `resetAt` RFC3339 |
| 9 | Kimi Code monthly sub-limit | `oauth/usage` `monthCode` | **Unsupported for our purpose** — must not substitute | Feature | Current period | `resetAt` RFC3339 |
| 10 | Extra Usage balance and monthly spend | `oauth/usage` `extraUsage` | **Unverified**, P1 | Account wallet | Current month | Unstated |
| 11 | Subscription expiry / renewal / auto-renewal | Subscription page | **Unverified** | Account | n/a | Unstated |
| 12 | Exact token counts for membership usage | none | **Unsupported by the source** | n/a | n/a | n/a |
| 13 | Absolute pool capacity in any unit | none | **Unsupported by the source** | n/a | n/a | n/a |
| 14 | A documented public JSON API for membership quota | none | **Unsupported by the source** | n/a | n/a | n/a |
| 15 | Per-feature consumption breakdown | Subscription page usage detail carries a feature field | **P1**, unverified | Per record | Last 1000 records | Unstated |

### Required metrics that no documented source can satisfy

- **Exact token consumption for membership usage.** The pool is exposed as a
  **ratio** (`usedRatio`, 0–1) and as consumption percentages in the usage detail.
  No token count is published. requirements.md forbids converting a percentage
  into tokens using model prices or a guessed plan capacity, so a cross-provider
  token total that includes Kimi is **not achievable** from documented sources.
  That limitation must be recorded in the UI rather than papered over, and an
  OpenRouter-only token total must never be presented as a total across all
  services.
- **Absolute capacity, consumption, or units.** Only when a source provides them —
  and none does. The honest display is a percentage with a clearly labelled
  direction (used vs remaining).
- **A date-range history query.** The subscription page exposes at most the last
  1000 usage-detail records with no documented range or cursor parameter. The
  interval those records actually cover varies with usage volume, so coverage must
  be reported as Partial with the observed interval stated — and a zero can never
  be asserted from it, because absence of records inside an unbounded window is
  not confirmation of no consumption.
- **Today's total.** Same problem, plus the documented delay: the usage detail
  "may be briefly delayed", and the current quota display is authoritative. Two
  sources disagreeing by design means today's figure is provisional at best.
- **A membership top-up or payment ledger.** Invoices exist (我的 → 订阅 → 额度和发票
  → 账单与发票) but no documented machine-readable purchase history.

### Percentage-point arithmetic

Because the only consumable quantity is a percentage, requirements.md's rule
applies directly: a move from 80% remaining to 70% remaining inside a **confirmed
unchanged** period is 10 percentage points of that period's allowance — not 10%
of some token total, and not 10/80 of the previous value. The confirmation step
is what makes this hard for Kimi: a monthly refresh, a plan upgrade (which
prorates remaining days and applies new benefits immediately), a gifted quota
grant, or an Extra Usage switch can all move the number without any consumption
occurring. Unless the period is confirmed unchanged via `resetAt` and the
subscription state, the value must be labelled **"Observed quota change"**.

## Error and transport semantics (Path B)

| Trigger | Documented result | Correct mapping |
| --- | --- | --- |
| Missing or bad bearer token | HTTP 401, envelope `code` 40101 | `ProviderError::PermissionRequired` |
| 10 auth failures in 60 s on a non-loopback binding | HTTP 429, envelope `code` 42901, 60-second ban | `ProviderError::RateLimited` with a 60-second retry |
| Provider not configured | envelope `code` 40110 | Permission/configuration failure, not a quota fact |
| Upstream account-service failure | **HTTP 200** with `data.kind == "error"`, optional upstream `status` | In-band failure; must not be read as zero quota |
| Stale pagination cursor | envelope `code` 40922 (`page_token` mismatch) | Restart the walk; never splice pages |
| Resource expired | envelope `code` 410xx | Treat as a state change, re-read |

The overriding rule: **HTTP status is almost always 200**. Branch on the envelope
`code`, then on `data.kind`, then on `data`. A transport-level 200 carrying
`kind: "error"` is a failed query, not an empty quota.

## Fixture inventory

Kimi fixtures live at [fixtures/kimi/](fixtures/kimi/) and are **shape-only**:
invented identifiers, invented ratios, invented timestamps, no cookies, no
authorization headers, no bearer tokens, no email, no phone number, no real
account data. See [fixtures/README.md](fixtures/README.md).

| File | Documents |
| --- | --- |
| `oauth_usage_ok_new_plan.json` | `oauth/usage` success for a new-style plan: `monthTotal` present, no `limit7d` |
| `oauth_usage_ok_legacy_plan.json` | Success with both `limit5h` and `limit7d`, plus `monthCode` |
| `oauth_usage_partial_windows.json` | Only `monthTotal` delivered — absent windows must be Missing, never synthesized |
| `oauth_usage_code_windows_only.json` | The substitution trap: `limit5h`, `limit7d`, and `monthCode` present and nearly exhausted, `monthTotal` **absent** — an adapter that grabbed any window would report a Kimi Code limit as the membership pool |
| `oauth_usage_upstream_error.json` | HTTP 200 with `kind: "error"` and an upstream `status` |
| `oauth_usage_extra_usage_null.json` | `extraUsage: null` — no Extra Usage enabled |
| `oauth_usage_extra_usage_active.json` | `extraUsage` populated, monthly cap enabled |
| `oauth_userinfo_shape_only.json` | Profile shape with PII fields **redacted by construction**, not merely invented |
| `envelope_unauthorized.json` | `code` 40101 |
| `envelope_rate_limited.json` | `code` 42901 |
| `subscription_page_observed_shape.md` | What the human-visible subscription page exposes, as a field list rather than JSON |

P2-03 owns the Kimi adapter and may keep its own adapter-local copies under
`crates/providers/src/kimi/fixtures/`. The corpus here is the documentation-grade
reference; nothing in `crates/providers/` was touched by P2-01.

## Open questions for live reconciliation (A03)

A real member must answer these against their own subscription page. None may be
assumed, and none may be answered from this document.

1. Does `oauth/usage` `monthTotal.usedRatio` match the percentage on the
   subscription page at the same instant, and to how many decimal places?
2. Does `monthTotal.resetAt` match the "next refresh time" the page displays, and
   in which zone is each expressed?
3. How large is the delay between a consumption event and its appearance in the
   usage detail, in practice?
4. Do the 1000 usage-detail records cover a fixed interval, and what is that
   interval for a heavy versus a light user?
5. Which windows does a real account actually receive — is `monthCode` present on
   non-Code plans, and is `limit7d` really absent on new plans?
6. Does `userLevelName` return the marketing tier name (Go/Plus/Pro/Max/Ultra, or
   a legacy name), and does it vary by region?
7. Is `extraUsage.currency` always CNY for mainland accounts, and what appears for
   other regions?
8. Does an exhausted pool report `usedRatio == 1.0` exactly, or can it exceed 1?
9. Does a plan upgrade change `resetAt` mid-period, and does the prorated refund
   move `usedRatio`?
10. Can the subscription page be reached with a user-authorized flow that does
    **not** require reading another tool's credentials — and if not, is the
    integration resolvable at all, or must scope be revised?

Question 10 is the feasibility gate. If the answer is "only via another tool's
local server token", the product owner must decide whether an explicit
user-authorized connection to that local server is acceptable, or whether Kimi
scope is revised. A manually entered quota **must not** stand in for a working
automatic integration.

## Honest status

| Criterion | Status | Why |
| --- | --- | --- |
| A01 provider adapters and capability reports | **Pending** | Documentation only. A01 requires a real Kimi shared membership pool. |
| A02 scope and product identity | **Pending** | The shared-pool evidence above is documentary; counting web/app/Code once has not been demonstrated against a real account. |
| A03 Kimi adapter validation | **Pending** | No comparison against an official member page has occurred. The retrieval path itself is unresolved. |
| G01 live Kimi access | **Open blocker** | No authorized retrieval path exists in this worktree, and none may be created by reading another tool's credentials. |

The simulated Kimi adapter in
[crates/providers/src/simulated.rs](../../crates/providers/src/simulated.rs)
remains the only working Kimi data source, and it is labelled
`Provenance::Synthetic`. Until Path A or Path B is authorized and reconciled, the
product must show **Not connected** rather than substituting an API wallet or an
independent Code limit.
