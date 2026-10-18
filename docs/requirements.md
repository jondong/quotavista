# QuotaVista Requirements Draft

Version: 0.8

Date: 2026-10-18

Status: Requirements review draft

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
| Settings | Refresh intervals, thresholds, and basic display options | English UI support, settings import and export |
| History and statistics | Persist usage history; daily, weekly, and custom-range totals, plan breakdowns, trends, coverage indicators, and restart recovery | Data export, consumption forecasting, and finer feature attribution |
| Desktop integration | macOS menu bar and Linux tray where available; retain a main-window entry point | Shortcuts and widgets |
| Data location | Credentials and monitoring data stay on the device | Explicitly enabled multi-device synchronization |

The initial UI is currently planned primarily in Simplified Chinese, with a structure that allows future English UI support. UI localization is separate from the English documentation policy. Linux scope means a graphical desktop; headless servers, a CLI, and remote collection are outside the first release.

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

Count each consumption once. Do not add official detail, official aggregates, and snapshot differences for the same usage. Repeated synchronization must update or deduplicate existing data, and provider revisions must be reflected without repeatedly increasing totals.

A balance change alone is not consumption. Quota percentage changes require checks for resets, upgrades, grants, and account adjustments. If these cannot be distinguished, label the result Observed quota change instead of verified consumption.

For example, a fall from 80% remaining to 70% within a confirmed unchanged quota period represents 10 percentage points of that period's allowance. It does not mean 10% of tokens or a 10% change relative to 80%. Keep reset periods separate, and do not add percentage points from plans with different capacities or rules into a single percentage.

Assign consumption by its occurrence time, not collection time. Do not arbitrarily distribute snapshot changes across days when they span midnight, a reset, or a period when the application was not running.

For each metric, show its source, coverage interval, last update, and complete, partial, missing, or unsupported status. Zero requires confirmation of no consumption within a complete source interval. Do not fill expired history, truncated records, failed queries, or collection gaps with zero.

Retain local history for 90 days by default. Longer custom ranges can be selected, but uncovered parts must be marked. Official backfill may recover usage while the application was stopped; unrecoverable snapshot gaps remain gaps. Allow later corrections to daily totals and label unsettled data.

An exact token total across all providers is a data-validation goal. If Kimi does not expose tokens, record that limitation during requirements and integration review. An OpenRouter-only token total must not be presented as an exact total across all services.

## Overview and account details

Use account cards with a consistent layout while retaining the native meaning of subscription and prepaid metrics.

Subscription cards emphasize remaining quota and refresh time; prepaid cards emphasize balance and low-balance status. Include the last successful synchronization time. Combine text and color for normal, attention, unavailable, and stale states.

If capacity is unknown, a provider-reported percentage can still be displayed. If neither a total nor a percentage exists, do not draw a progress bar. Do not invent a denominator for a wallet balance.

Details show native accounting units, source times, known plan rules, connection state, and a link to the official console. Normal user flows do not expose endpoint paths or debugging internals.

The empty state should offer a clear connection action. No data, failed queries, insufficient permissions, and a real zero balance must be distinct.

## Refresh and data trust

- Default automatic refresh to five minutes; offer one minute, five minutes, fifteen minutes, or disabled.
- Attempt a refresh on launch, wake, or network recovery within allowed request rates. Do not promise collection while the device sleeps or the application is stopped.
- Support refresh for one or all connections; merge overlapping requests.
- Failure for one account must not block updates for another.
- Retain last successful values on failure, with the reason and last success time; do not replace them with zero.
- Initially mark data stale after two unsuccessful refresh intervals. When automatic refresh is disabled, keep the data timestamp prominent.
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

## Decisions before requirements are finalized

| Decision | Status |
| --- | --- |
| Prepaid provider | OpenRouter confirmed; OpenCode Zen excluded from the first release |
| Kimi scope | Shared membership quota based on the user's description; independent Code limits are P1 |
| Exact Kimi plan and rules | Confirm before integration validation; do not put credentials into the document |
| Range statistics | Required in P0; exact Kimi tokens still need validation |
| P0 and P1 split | Pending review |
| Linux environment | Ubuntu 26.04, GNOME, x86_64 confirmed; compatibility not yet validated |
| macOS environment | Version and CPU architecture to be confirmed |
| Documentation language | English confirmed for all project documentation |

## Next steps

Continue reviewing this requirements draft under docs. The MIT License is in place. Follow the [architecture](architecture.md) and [development plan](development-plan.md). Token-availability research remains deferred to provider integration.

Architecture decisions must address collection, authorization, different units, scheduling, credential storage, extensibility, and platform packaging. Evaluate independent implementation against the cost and constraints of reusing existing open-source components.

Reliable provider access is a prerequisite for a complete implementation. If validation changes a required capability, explicitly revise requirements before treating the integration as complete.
