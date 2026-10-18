# QuotaVista Requirements Draft

Version: 0.9

Date: 2026-10-18

Status: Reviewed draft for prototype planning; provider and product decisions remain gated

Project: QuotaVista

Sequence: Define requirements -> Choose a project name -> Design the architecture -> Initialize the application -> Implement and validate

## Product goal

Provide individual users of multiple AI services with a clear desktop dashboard for subscription quotas, prepaid balances, token consumption, and per-plan usage over daily, weekly, and custom periods. Help users recognize low quotas, low balances, and approaching subscription expiration.

The first release covers a subscription service and a prepaid usage-based service. Its purpose is monitoring and alerts; it does not make model requests, process payments, or route inference traffic.

## Established direction

| Area | Requirement |
| --- | --- |
| Audience | Individual developers and other users of multiple AI services |
| Initial subscription provider | Kimi shared membership quota; confirm the exact plan during integration |
| Initial prepaid provider | OpenRouter prepaid accounts, charged according to usage |
| Initial platforms | macOS and Linux desktops; Linux target: Ubuntu 26.04, GNOME, x86_64 |
| Future platforms | Preserve room for Windows support; no Windows installer is required for the first release |
| Open-source project | Public repository jondong/quotavista, licensed under MIT |
| Documentation language | English for all project documentation, including diagrams and contribution guidance |
| Current phase | Name and license established; architecture and development plans available; proceed through prototype and integration validation |

## Integration scope

The first integrations cover Kimi's shared membership quota and OpenRouter prepaid accounts. OpenCode Zen is a separate service and is outside the first release.

Based on the user's description, Kimi monitoring targets the shared membership pool. Confirm the plan name and quota rules during integration without putting credentials into requirements discussions. Independent Kimi Code short-term or weekly limits are P1. Code usage charged to the shared pool is part of that pool and must not be counted again.

## User tasks

| Situation | What the user needs to know |
| --- | --- |
| Before using Kimi membership features | How much shared quota remains and when it refreshes |
| Shared quota is running low | Whether it is close to exhaustion and when it next refreshes |
| Using a prepaid service | How much balance remains and whether to top up |
| Routine check | Which account needs attention and whether its data recently synchronized |
| Reviewing a period | Total recorded tokens and consumption by plan or account for a day, week, or other range |
| Subscription approaching expiration | Expiration date and known auto-renewal status |
| Network or authentication failure | Whether values are stale and whether to retry, check the network, or reconnect |

After setup, the overview should let users assess available quota and balance without opening every provider console.

## Proposed first-release scope

P0 requirements are required for the first release. P1 items are candidates for later releases and are not first-release commitments. Open decisions are not treated as approved scope.

| Area | P0 | P1 |
| --- | --- | --- |
| Providers | Kimi shared membership quota and OpenRouter prepaid accounts | GLM, Qwen, MiniMax, DeepSeek, OpenAI, SiliconFlow, and others |
| Connections | At least one account per provider; rename, update credentials, disable, and remove | Multiple accounts per provider and bulk import |
| Overview | Quota, balance, selected-period consumption, update times, and errors on one screen | Custom layouts and additional summaries |
| Quotas | Kimi shared quota used or remaining percentage and available refresh time | Independent Code limits and usage by feature or model |
| Wallets | Account balance and period consumption; distinguish account balance from key budget | Top-up details, full bill reconciliation, and cost attribution |
| Refresh | Automatic and manual refresh; retain last successful values | More advanced refresh policies |
| Alerts | In-app alerts and configurable system notifications | Additional notification channels |
| Settings | Refresh intervals, thresholds, basic display options, English and Simplified Chinese UI selection | Other UI locales, settings import and export |
| History and statistics | Persist usage history; daily, weekly, and custom-range totals, plan breakdowns, trends, coverage indicators, and restart recovery | Data export, consumption forecasting, and finer feature attribution |
| Desktop integration | macOS menu bar and Linux tray where available; retain a main-window entry point | Shortcuts and widgets |
| Data location | Credentials and monitoring data stay on the device | Explicitly enabled multi-device synchronization |

The first release must include both English and Simplified Chinese interfaces, as confirmed by the user. Documentation is always English. Linux scope means a graphical desktop; headless servers, a CLI, and remote collection are outside the first release.

## Interface languages

Ship English (en) and Simplified Chinese (zh-CN) catalogs with the application; no network translation service is required. On first launch, choose Simplified Chinese for an explicitly Simplified Chinese system locale and English otherwise. An explicit user selection overrides system detection and persists across restarts.

Language switching applies immediately to navigation, settings, empty states, errors, charts, accessible labels, and subsequent system notifications. Preserve the current screen, date filters, selected accounts, and entered nonsecret form data. Store alert types and parameters rather than translated strings; changing language must not send the alert again.

Use locale-aware display formatting for dates and numbers while keeping the reporting time zone, currency, underlying decimal values, provider identities, and statistics unchanged. Language does not select a different currency or time zone. User aliases and official provider or model names are not automatically translated.

Missing runtime translations fall back to English. Before release, both catalogs must contain every required P0 key; fallback is a safety mechanism, not permission to ship an untranslated critical flow. Validate complete connection and statistics flows in both languages. Additional locales remain P1.

## Connection lifecycle and user flows

The P0 interface needs one active connection for each initial provider. Multiple simultaneous accounts per provider remain P1. Internally, retain distinct account scopes so replacement credentials cannot silently combine unrelated history.

| Flow | Required behavior | Completion or failure |
| --- | --- | --- |
| Add a connection | Choose provider, alias, credential type, and persistent or explicitly chosen session-only storage; explain permissions before validation | Save a usable connection only after a metadata query validates the credential or returns a specific partial-permission result; no inference request |
| Partial authorization | List available and unavailable metrics separately | Keep useful key data, but do not label account balance connected when it is unavailable |
| Rename | Change only the local alias | Preserve identity, credentials, history, and alert episodes |
| Disable | Stop scheduling and cancel in-flight work | Retain local data and credentials, show Disabled, and do not accept late results |
| Re-enable | Resume permitted collection using the same scope | Cached values keep their original timestamps until fresh data arrives |
| Update credentials | Validate the candidate without destroying a working credential | Preserve history only when the provider account and workspace identity match; an unverified or different identity needs a separate scope and explicit replacement flow |
| Remove | Explain that credentials, history, and managed backups will be removed; stop collection immediately | Complete only after cleanup succeeds; otherwise show Deletion pending with retry, including after restart |
| Restart with session-only access | Restore nonsecret configuration and history | Show Needs credentials, not Expired credential, and do not collect until the user reconnects |

Reject an empty credential or alias. Trim surrounding alias whitespace, preserve credential contents, limit aliases to 80 characters, and never echo credentials in validation errors. A rejected candidate must not replace an active credential. No default browser-cookie import, automatic top-up, or account administration is part of these flows.

### Separate state dimensions

Connection lifecycle, query health, data freshness, and historical coverage are independent. For example, a connection can be active with an unavailable balance capability, a fresh quota value, and partial history.

- Lifecycle: Pending setup, Active, Disabled, Needs credentials, or Deletion pending.
- Query health per dataset: Not queried, Running, Success, Offline, Timeout, Rate limited, Authentication required, Permission required, Invalid response, or Local storage error.
- Value freshness: No value, Current, or Stale, always with collection time and source reporting time when known.
- Historical coverage per metric: Complete, Partial, Missing, or Unsupported, with reasons such as source retention, truncation, a collection gap, or pending settlement.

An authentication failure must not hide cached values. A database failure must not mark an unsaved response as successfully synchronized. A successful quota request must not clear an unrelated history error.

## Kimi shared membership quota

### Displayed information

- Account alias, provider, and connection state.
- Plan name; show unknown if it cannot be identified, rather than guessing from quota size.
- Remaining shared quota percentage. If only used percentage is available, derive the remainder and label the direction clearly.
- Quota capacity, consumption, and units when provided. Do not invent token or request counts from a percentage.
- Current quota period and next refresh time. Do not infer an exact refresh from payment dates when the provider does not supply one.
- Subscription expiration, renewal date, and auto-renewal status when available.
- Shared quota consumed during the selected date range; finer feature breakdowns are P1.

### Rules

Kimi describes a shared membership pool that refreshes monthly according to the subscription period for both monthly and annual subscriptions. Unused quota does not roll over. Code may also have independent limits depending on the plan. [Official membership description](https://www.kimi.com/help/membership/membership-overview)

Monitor the shared pool as one unit. Usage from the web app, mobile app, or Code that draws on this pool is counted once. Payment frequency, membership validity, and quota refresh are distinct; annual payment does not imply annual quota refresh.

A remaining shared quota percentage does not guarantee that every feature, including Code, is currently available. Independent feature limits are outside P0, so the application must not infer that all features are usable.

Kimi Open Platform API credit balances and independent Code limits cannot substitute for membership quota. Extra Usage purchases, consumption, and transaction details are P1. If the source explicitly exposes extra-usage status, distinguish it from included subscription quota.

### Integration feasibility gate

Compare automatically retrieved shared quota and refresh times with a real member's official subscription page. The web and mobile apps expose these values, but automated third-party retrieval and authorization still need validation. Retrieving only independent Code limits does not satisfy this integration. [Official lookup guidance](https://www.kimi.com/help/membership/membership-overview)

If access requires a login session, the user must authorize a clear connection flow. Do not scan or import browser cookies or read other tools' credentials by default. Explain credential sources and necessary permissions during connection.

If reliable shared-quota data cannot be retrieved, resolve the integration or explicitly revise scope. Manually entered quota must not stand in for a working automatic integration.

## OpenRouter prepaid accounts

Monitor the Credits remaining for usage-based requests and clearly distinguish account balance from the budget of an individual API key.

### Displayed information

- Account alias, provider, and connection state.
- Remaining account Credits, in the provider's accounting currency.
- Cumulative purchased Credits and usage returned by the official interface as detail fields.
- Tokens, input and output breakdowns, and actual consumption for the selected period.
- Key consumption, limits, and remaining budget when current-key information is connected.
- Explicit account-level or key-level scope so a shared balance is not counted once per key.

### Permissions and calculations

The official Credits endpoint requires a Management Key and returns total_credits and total_usage. Their difference can represent remaining Credits; validate the result, permission, and accounting scope against the console. [Credits endpoint](https://openrouter.ai/docs/api/api-reference/credits/get-remaining-credits)

A regular inference key can query current-key information, but its budget and remaining allowance are not the entire account balance. An unlimited key budget does not imply unlimited account funds. [Current-key endpoint](https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key)

If only a regular key is available, display the supported information and identify the missing account-balance permission. This does not satisfy acceptance for account-balance monitoring.

A Management Key has administrative capabilities beyond an inference key. Explain its intended use, perform only the required queries, and do not create, modify, or delete keys or other account resources.

Top-up payments, fees, usable Credits, and consumed Credits are distinct. Do not label cumulative purchased Credits as the user's total cash expenditure.

## Consumption over time

### Date ranges and views

Provide Today, Yesterday, This week, Last week, This month, Last 7 days, Last 30 days, and custom date ranges. Support daily or weekly aggregation, range totals, per-plan or account breakdowns, and trends. The overview can include the selected range's summary.

This week begins on Monday. Last 7 days includes today and the previous six calendar days. Mark the current unfinished day as provisional or in progress. Show explicit start and end dates and the reporting time zone; do not confuse a calendar week with the last seven days.

Default to the system time zone and allow users to inspect and select the reporting zone. Convert to local calendar boundaries only when source granularity supports it. Display UTC daily buckets as UTC rather than proportionally splitting them into local days. Do not combine incompatible time boundaries into one daily total.

The date picker uses inclusive calendar dates; calculations use a start-inclusive, end-exclusive interval. A selected day ends at the next calendar midnight in the reporting zone, not at a fixed offset of 24 hours. Clip collection for every still-open range at a captured as-of instant, including Today and custom ranges ending today; mark the result provisional. Time after that instant is future time, not a collection gap. Reject reversed ranges, unknown time zones, invalid dates, or end dates after today. The initial custom-range limit is 366 calendar days; older ranges within that limit remain selectable with explicit coverage gaps.

If a provider cannot support the requested local-day boundaries, show its native UTC series separately and exclude incompatible values from a common total. Explain the exclusion and offer an explicit UTC view. For a week selection, sum only complete source buckets contained in the selected interval; a bucket crossing an edge is excluded and that edge remains partial. A successful request must not silently change the user's date range or imply incompatible series share the same boundaries.

### Metrics and plan breakdowns

| Metric | Consumption within the range | Presentation |
| --- | --- | --- |
| Total tokens | Available input and output counts with compatible counting semantics | Separate input and output; identify included accounts, plans, and missing coverage |
| Kimi plan consumption | Actual shared membership quota consumed | Prefer official usage records; use percentage points when only quota percentages are available |
| OpenRouter consumption | Tokens and actual Credits or monetary consumption | Separate from top-ups, current balance, and key budgets |
| Per-plan or account detail | Provider, account, plan, and native unit per row | Show available native consumption even when tokens are unavailable |

OpenRouter is a prepaid wallet. It can appear alongside subscriptions under plans and accounts, without inventing a subscription tier. Even with one account per provider, show separate Kimi and OpenRouter consumption for the same selected range.

When a plan does not expose tokens, label the aggregate as Recorded tokens and identify excluded plans and reasons. Do not present it as an exact total across every plan. Token counts across models are recorded usage totals, not a measure of equivalent cost or work.

Check how each source defines input, output, cached, and reasoning tokens. Do not add cached or reasoning counts again if they are already included in input or output. Avoid a potentially duplicated total when the relationships are unknown.

### Sources and available coverage

OpenRouter's Activity endpoint includes input, output, cached, and reasoning tokens and consumption, covering the last 30 completed UTC days with a Management Key. Analytics also supports time-range queries. Validate current-day availability, metric semantics, granularity, and truncation during integration. [Activity endpoint](https://openrouter.ai/docs/api/api-reference/analytics/get-user-activity-grouped-by-endpoint), [Analytics endpoint](https://openrouter.ai/docs/api/api-reference/analytics/query-analytics-data)

An endpoint containing only completed UTC days cannot establish a complete live total for today. Keep the Today view and explain unavailable data instead of showing zero.

Kimi's subscription page provides quota percentages, refresh times, and recent usage details. Automated access to consumption, historical coverage, and exact tokens needs validation. Do not convert quota percentages into tokens using model prices or a guessed plan capacity. [Official membership description](https://www.kimi.com/help/membership/membership-overview)

Prefer official records or time buckets and backfill as far as the provider allows. Data observable only after local collection begins must not be presented as a complete history before connection.

### Calculation and completeness

Count each consumption once. Do not add official detail, official aggregates, and snapshot differences for the same usage. Repeated synchronization must update or deduplicate existing data. Provider revisions, including records removed from a later complete authoritative response, must replace the previous contribution without repeatedly increasing totals. An incomplete replacement must not erase an earlier valid result or claim current completeness.

A balance change alone is not consumption. Quota percentage changes require checks for resets, upgrades, grants, and account adjustments. If these cannot be distinguished, label the result Observed quota change instead of verified consumption.

For example, a fall from 80% remaining to 70% within a confirmed unchanged quota period represents 10 percentage points of that period's allowance. It does not mean 10% of tokens or a 10% change relative to 80%. Keep reset periods separate, and do not add percentage points from plans with different capacities or rules into a single percentage.

Assign consumption by its occurrence time, not collection time. Do not arbitrarily distribute snapshot changes across days when they span midnight, a reset, or a period when the application was not running.

For each metric, show its source, coverage interval, last update, and complete, partial, missing, or unsupported status. Zero requires confirmation of no consumption within a complete source interval. Do not fill expired history, truncated records, failed queries, or collection gaps with zero.

Retain local history for 90 days by default. Longer custom ranges can be selected, but uncovered parts must be marked. Official backfill may recover usage while the application was stopped; unrecoverable snapshot gaps remain gaps. Allow later corrections to daily totals and label unsettled data. Unfinished current-day data is provisional even when all currently available records were fetched.

At a retention boundary, preserve an indivisible source bucket that overlaps the cutoff and state its exact interval; do not split its value. Purging data must also reduce its coverage. The latest current value remains available independently of historical retention. A retention task must never remove an unsettled synchronization checkpoint needed to recover a failed transaction.

An exact token total across all providers is a data-validation goal. If Kimi does not expose tokens, record that limitation during requirements and integration review. An OpenRouter-only token total must not be presented as an exact total across all services.

## Overview and account details

Use account cards with a consistent layout while retaining the native meaning of subscription and prepaid metrics.

Subscription cards emphasize remaining quota and refresh time; prepaid cards emphasize balance and low-balance status. Include the last successful synchronization time. Combine text and color for normal, attention, unavailable, and stale states.

If capacity is unknown, a provider-reported percentage can still be displayed. If neither a total nor a percentage exists, do not draw a progress bar. Do not invent a denominator for a wallet balance.

Details show native accounting units, source times, known plan rules, connection state, and a link to the official console. Normal user flows do not expose endpoint paths or debugging internals.

The empty state should offer a clear connection action. No data, failed queries, insufficient permissions, and a real zero balance must be distinct.

## Refresh and data trust

- Default automatic refresh to five minutes; offer one minute, five minutes, fifteen minutes, or disabled.
- When automatic refresh is enabled, attempt a refresh on launch, wake, or network recovery within allowed request rates. With automatic refresh disabled, only explicit connection validation or manual refresh may contact providers. Do not promise collection while the device sleeps or the application is stopped.
- Support refresh for one or all connections; merge overlapping requests.
- Failure for one account must not block updates for another.
- Retain last successful values on failure, with the reason and last success time; do not replace them with zero.
- Mark a current-value metric stale when elapsed time since its last successful collection reaches twice the configured refresh interval, regardless of missed timers or failure counts. In manual-only mode, use ten minutes for this indicator and show Automatic refresh off. Source reporting delay is a separate label; repeatedly fetching old historical data must not refresh a current-value timestamp.
- Respect provider rate limits and retry guidance instead of polling rapidly after failure.
- Record collection time and source reporting time separately. Do not present collection time as a provider accounting timestamp.
- Make absolute timestamps and time zones available for relative times; calculate periods using provider rules.
- Real-time monitoring means periodic refresh and freshness indicators while the app runs, not per-request or second-by-second synchronization.

## Alerts

| Alert | Proposed trigger | Required information |
| --- | --- | --- |
| Low quota | Kimi shared quota first falls to the configured threshold, default 20% | Identify the shared pool and known refresh time |
| Quota exhausted | The source explicitly reports exhaustion | Distinguish from network or service failure |
| Low balance | Balance first falls to the user's monetary threshold | Show currency and amount; the user sets the threshold |
| Expiration approaching | A known expiration date enters the reminder window, default three days | Distinguish expiration, renewal, and quota refresh |
| Connection invalid | Authentication is clearly invalid or permissions are insufficient | Offer reconnection or permission correction |

Evaluate new alerts using fresh successful values. Stale cached values do not trigger new quota or balance alerts. Repeated polls must not resend the same alert. Allow a new alert after recovery, and prevent duplicates within the same period across application restarts.

Only validated live current-value queries are eligible for quota or balance notifications. A response explicitly identified as an expired period, delayed historical result, or stale upstream cache is not eligible even if it was fetched just now. When a current endpoint omits source timestamps, disclose the unknown source time; do not invent one. Capability validation must define how each source establishes current-value eligibility.

The first fresh live sample at or below a threshold creates one alert, including a connection first added while already low. Recovery means a later fresh value strictly above the threshold; the next crossing can then create a new episode. A provider-confirmed new quota period also starts a new episode, but local midnight does not. Exhaustion takes precedence over low quota in the same update, producing one notification with the more severe state.

Cached startup values, historical backfill, renames, and opening a page never generate a new notification. Threshold edits update the in-app state immediately, but desktop delivery waits for a new live sample and must not reset an already-delivered episode. Expiration alerts use the actual expiration instant, not a guessed renewal date; renewed subscriptions rearm only when the source confirms a new validity period.

OS notification delivery is best effort and cannot be guaranteed exactly once after a crash. Deduplication must favor avoiding repeat delivery; in-app state remains available even when a system notification is lost or denied.

System notifications can be disabled. Default notification content must not include account identity or credentials. Quota-restored notifications are P1; refresh information remains visible in the overview.

## Units and accounting

Keep subscription fees, top-ups, usage charges, granted quota, and key budgets distinct. Do not calculate a global remaining percentage or directly add CNY and USD.

Do not value Kimi quota percentages using API token prices. OpenRouter balance changes do not directly establish period spending because top-ups and account adjustments also affect balances.

Distinguish official consumption, validated derived values, and estimates. Missing, unsupported, unauthorized, stale, and numerical zero are separate states.

Usage history, daily and weekly trends, range token totals, and per-plan consumption are P0. Full bill reconciliation, subscription-fee amortization, currency conversion, and exhaustion forecasting remain P1.

## Privacy and local data

- No QuotaVista cloud account is required.
- Store keys, session tokens, and monitoring data locally by default. Connect only to enabled providers; do not send usage to the project author.
- Use supported OS credential storage. Do not silently save plaintext credentials when Linux lacks a usable store; the architecture defines the alternative.
- Do not collect prompts, inference request bodies, or model responses. No telemetry by default.
- Credentials must never appear in logs, diagnostics, errors, or public demonstrations.
- Removing a connection deletes its saved credentials and local monitoring data, with the consequences stated in the action.
- The user initiates official-console login or reauthentication. The application does not purchase, renew, or top up services.

Session-only storage requires explicit selection, keeps credentials in backend memory, and requires reconnection after process exit. A locked credential store must not trigger a silent downgrade to session-only or plaintext storage.

Managed migration backups containing removed data must also be deleted before removal completes, even when that means deleting a full-database backup that also contained other accounts. A cleanup failure remains visible and resumable. Deletion covers application-managed data and secrets, not provider records, user-created copies, OS backups, or forensic erasure of physical media. Restoring a managed backup must not silently resurrect a deletion that was already acknowledged.

Local usage history is not promised to be encrypted at rest. Explain the distinction between encrypted system credential storage and local usage metadata protected by the user's OS account. No first-release export or automatic backup upload is included.

## Input and operating limits

These are proposed prototype budgets, not measured results. Validate them against the final support matrix and revise them explicitly if evidence requires a change.

| Area | Initial budget or rule | Validation |
| --- | --- | --- |
| Local statistics | P95 at most one second for a 90-day query over 100,000 synthetic usage facts, without network calls | Record machine, build, dataset, query mix, and timing after five warm-up queries and at least 30 measured queries |
| Interaction | Show loading state immediately; network and storage work must not block navigation | Validate while collection is slow or rate limited |
| Network work | Bounded request duration, response size, pagination, retries, and concurrency | Slow and oversized response fixtures; reaching a limit gives Partial or an error, never false completeness |
| Numeric integrity | Reject invalid ratios, integer overflow, unsupported decimal precision, and incompatible units | Boundary fixtures; never round silently into a valid-looking bill |
| Local failure | Preserve committed data after disk-full, migration, or write failure | Failure injection and restart; no false success indicator |
| Accessibility | Keyboard-accessible essential flows and textual status equivalents for color | Connection, overview, statistics, and alert checks on supported desktops |

Default to OS proxy settings if the runtime can support them consistently, document actual behavior, and provide a way to reinitialize network access after proxy changes. Proxy configuration must not require disabling TLS verification or putting proxy secrets into diagnostics. Custom per-provider proxy configuration remains outside the first release.

## Platforms and open-source delivery

Both macOS and Linux must support the main window, connections, refresh, core metrics, and alerts. Linux must remain usable through the main window when a tray is unavailable.

The confirmed Linux target is Ubuntu 26.04, GNOME, x86_64. Record the actual session type during validation. Confirm macOS versions, CPU architectures, and package formats in the support matrix. Validate actual behavior in supported environments; successful compilation alone is insufficient.

All project documentation must be in English for a global audience, including README files, requirements, architecture, development and installation instructions, contribution guidance, and diagram labels. Keep documentation in English as the project evolves.

Open-source delivery includes the README, requirements, architecture, installation and development guidance, license, contribution guidance, and examples without real credentials. The project uses MIT; reused third-party code must retain the notices required by its license.

The project is named QuotaVista, its local directory is quotavista, and its public repository is jondong/quotavista. Published materials currently include project documentation and the MIT License. Implementation decisions are described in [architecture.md](architecture.md).

## Explicit first-release exclusions

- Model chat, inference routing, local proxies, and changes to other coding tools' settings.
- Automatic top-ups, renewals, model switching, or control of external requests.
- Team permissions, reimbursements, and full financial accounting.
- Cloud accounts, device synchronization, mobile apps, and headless Linux administration.
- All-provider integration at once or an executable third-party plugin marketplace.
- Full historical bills, a model price catalog, cost-optimization advice, and exhaustion forecasts.
- A production Windows release.

## Acceptance criteria

| ID | Scenario | Passing condition |
| --- | --- | --- |
| A01 | Two billing models | Automatically retrieve core data from a real Kimi shared membership pool and OpenRouter prepaid account; demo data does not qualify |
| A02 | Kimi shared-pool scope | Show the membership pool without substituting an API wallet or independent Code limit; do not duplicate usage across surfaces |
| A03 | Kimi reconciliation | Match the official page for the same period, except for explained reporting delay or display precision |
| A04 | Account and key scope | Separate OpenRouter account balance and key budget; no invented balance when permission is missing |
| A05 | Real zero | Display real zero or exhaustion distinctly from unknown and failed requests |
| A06 | Refresh failure | Handle offline, timeout, authentication, and rate limits; retain last values and allow other connections to update |
| A07 | Freshness | Show last successful time and staleness; attempt an allowed refresh after wake |
| A08 | Alerts | Validate threshold crossings and deduplication with known data or fixtures without spending down paid accounts |
| A09 | Restart recovery | Restore configuration and last results with their original timestamps |
| A10 | Privacy | No credentials in plaintext configuration, logs, errors, examples, or version control |
| A11 | Cross-platform behavior | Validate connections, overview, refresh, and alerts in the supported macOS and Linux matrix |
| A12 | Independent use | All P0 features work without GitHub sign-in or a QuotaVista account |
| A13 | Time ranges | Support all specified presets and custom dates with explicit daily and weekly boundaries and zones |
| A14 | Token semantics | Sum compatible input and output counts without duplicating cached or reasoning tokens; incomplete coverage is not labeled complete |
| A15 | Plan breakdowns | Show Kimi quota consumption and OpenRouter tokens and charges for the same range, with units and coverage |
| A16 | Historical correctness | Repeated sync does not duplicate usage; resets, top-ups, midnight, and zone differences do not create false consumption or attribution |
| A17 | Missing data | Validate absent tokens, UTC-only buckets, unsettled days, truncation, and collection gaps with fixtures; never fill gaps with zero; reconcile available real-provider intervals |
| A18 | Connection lifecycle | Validate add, partial permission, disable, re-enable, failed rotation, identity mismatch, session-only restart, and deletion-pending recovery without cross-account history mixing |
| A19 | Authoritative revisions | A complete replacement can update or remove prior facts; incomplete pages do not replace a valid published partition or advance completeness |
| A20 | Retention and deletion | Purge facts with matching coverage, retain latest values, delete managed backup copies on removal, and prevent acknowledged deletions from returning during recovery |
| A21 | Credential and IPC boundaries | Session-only credentials never reach disk; locked stores remain explicit; untrusted callers cannot access privileged commands or read credentials |
| A22 | Validation and bounded work | Invalid dates, zones, aliases, numbers, oversized responses, and pagination limits fail explicitly; no fake zero or complete result |
| A23 | Local failure and usability | Measure the local-statistics budget; navigation stays usable during slow requests; storage errors preserve committed data and do not show false sync success |
| A24 | English documentation | README, requirements, architecture, plans, contributor guidance, and diagram labels are in English; required local links resolve |
| A25 | Bilingual interface | Ship complete en and zh-CN P0 catalogs; verify first-launch selection, immediate switching, persisted preference, fallback, notifications, and formatting without changing units, time zone, statistics, or alert episodes |

## Decisions before requirements are finalized

| Decision | Status |
| --- | --- |
| Prepaid provider | OpenRouter confirmed; OpenCode Zen excluded from the first release |
| Kimi scope | Shared membership quota based on the user's description; independent Code limits are P1 |
| Exact Kimi plan and rules | Confirm before integration validation; do not put credentials into the document |
| Range statistics | Required in P0; exact Kimi tokens still need validation |
| P0 and P1 split | Refined and internally reviewed for prototype planning; final product and release approval is separate |
| Linux environment | Ubuntu 26.04, GNOME, x86_64 confirmed; compatibility not yet validated |
| macOS environment | Development host observed as macOS 27.0.1 arm64; minimum supported version and release matrix remain open |
| Documentation language | English confirmed for all project documentation |
| UI languages | User confirmed English and Simplified Chinese in the first release; use system detection initially and persist explicit selection |

## Review and validation gates

The [requirements review](reviews/requirements-review.md) records resolved design findings. This is an internal document review, not independent certification, provider validation, or user approval.

| Gate | Required evidence | Blocks |
| --- | --- | --- |
| G01 | Kimi shared-membership authorization and core quota reconciliation | Real Kimi integration and release, not simulated prototyping |
| G02 | Provider token semantics, history, current-day reporting, and permissions | Claims of exact cross-provider token coverage and corresponding release criteria |
| G03 | User confirmed both English and Simplified Chinese; validate both complete catalogs and flows during implementation | Product language choice is closed; bilingual runtime acceptance remains required before release |
| G04 | macOS release matrix and actual Linux session recorded, with behavior verified | A supported-platform release claim |

No unresolved documentation-level correctness blocker remains within this review's scope. G01, G02, and G04 remain open and must not be converted to Passed by document checks or synthetic data. G03's product decision is closed; A25 still requires implementation evidence.

## Next steps

Continue reviewing this requirements draft under docs. The MIT License is in place. Follow the [architecture](architecture.md) and [development plan](development-plan.md). Token-availability research remains deferred to provider integration.

Architecture decisions must address collection, authorization, different units, scheduling, credential storage, extensibility, and platform packaging. Evaluate independent implementation against the cost and constraints of reusing existing open-source components.

Reliable provider access is a prerequisite for a complete implementation. If validation changes a required capability, explicitly revise requirements before treating the integration as complete.
