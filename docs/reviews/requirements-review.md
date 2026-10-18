# Requirements Review

Date: 2026-10-18

Reviewed version: Requirements 0.9

Method: Internal document review by the authoring assistant, followed by a separate consistency pass. No independent reviewer or runtime validation is implied.

## Verdict

The refined requirements are ready to guide a bounded prototype. No unresolved documentation-level correctness blocker remains in the reviewed scope. This is not final product approval or a finding that provider integrations are feasible.

The review preserves Kimi shared membership quota, OpenRouter prepaid accounts, time-range statistics, macOS and Ubuntu 26.04 GNOME x86_64, English documentation, MIT licensing, and deferred token-availability research. It does not add live provider investigation or application code.

## Review criteria

- Preserve confirmed user scope and distinguish assumptions from decisions.
- Make current values, consumption, units, time boundaries, and missing data unambiguous.
- Describe normal, partial, failed, and restart behavior for each essential flow.
- Give each required behavior a testable acceptance condition.
- Identify source-dependent promises and platform validation gates explicitly.
- Keep requirements about user-visible outcomes separate from implementation mechanics.

## Findings and disposition

Severity describes the consequence of leaving the earlier wording unchanged. Resolved means the specification was corrected, not that the software behavior has been implemented or tested.

| ID | Severity | Finding | Resolution and acceptance |
| --- | --- | --- | --- |
| RQ01 | High | Credential updates always preserved history, even if the replacement key belonged to another account | Require verified account and workspace identity before preserving history; otherwise use a separate scope and explicit replacement. A18 |
| RQ02 | High | Upsert-only history rules did not handle records removed by a later authoritative response | Complete replacements must remove superseded contributions; partial replacements retain previous valid results. A19 |
| RQ03 | High | Deletion did not define whether managed migration backups retained the removed data | Delete containing managed backups before acknowledging removal; make failed cleanup resumable and define external-copy exclusions. A20 |
| RQ04 | High | UTC-only sources could be silently mixed with local-day series or partially intersecting buckets | Preserve native boundaries, exclude incompatible edges, and expose partial coverage or an explicit UTC view. A13, A17 |
| RQ05 | Medium | Staleness depended on failed polls instead of elapsed time, and automatic-refresh-off behavior was ambiguous | Use per-metric elapsed age, separate source latency, and prevent implicit provider requests when polling is disabled. A06, A07 |
| RQ06 | Medium | First-low samples, threshold edits, severity conflicts, and expiration rearming were underspecified | Define alert episodes and precedence; one initial live alert, no cache/backfill delivery, no dedup reset on view or configuration changes. A08 |
| RQ07 | High | Global token totals could be read as complete despite excluded plans or provisional current days | Name included and excluded scopes, preserve provisional status, and keep exact-provider coverage behind G02. A14, A15, A17 |
| RQ08 | Medium | The Chinese-first UI assumption was not a confirmed choice for the global project | The user confirmed English and Simplified Chinese for P0 during this review. Add locale selection, complete catalogs, formatting invariants, and A25; retain English-only documentation. G03, A24 |
| RQ09 | Medium | Core connection states, session-only restart, storage failure, and operating limits lacked acceptance conditions | Add lifecycle flows, independent state dimensions, input rules, proposed performance budgets, and A18 through A23 |
| RQ10 | Medium | Current-period ranges could request future time or report it as missing history | Clip collection at one captured as-of instant and mark the still-open period provisional. A13, A17 |

The final consistency pass also distinguished collection freshness from upstream current-value eligibility: a newly fetched historical or expired-period response cannot create a live balance/quota notification. This refines RQ05 and RQ06 rather than introducing a new provider capability assumption.

## Scenario review

The refined wording was checked against these scenarios; they are design walkthroughs, not executed application tests.

| Scenario | Required conclusion |
| --- | --- |
| Initial live shared quota is 15% remaining | Create one low-quota episode; later identical polls do not notify again |
| Cached 15% value appears after restart while offline | Show stale cached data and the prior episode, without a new live alert |
| Ordinary OpenRouter key lacks balance permission | Show any available key metrics, label missing account permission, and fail A01 account-balance completion |
| Candidate key identifies a different workspace | Do not stitch its consumption to the existing workspace |
| A local-day query intersects a UTC-only bucket | Do not proportionally split or claim complete local-day coverage |
| A later complete response removes a previously returned record | Reduce the authoritative contribution once; a partial page cannot remove it |
| Deletion cannot unlock the credential store | Stop collection and show Deletion pending rather than announcing success |
| Provider returns no exact Kimi tokens | Show native quota consumption and an explicitly incomplete recorded-token total; keep G02 open |

## Open gates

| Gate | Status | Closure evidence |
| --- | --- | --- |
| G01 Kimi authorization and quota access | Open, deferred to integration | Real account comparison and an authorized connection flow |
| G02 Token and history capability | Open, deferred to integration | Provider capability report and data reconciliation; no guessed tokens |
| G03 UI languages | Product choice closed: both English and Simplified Chinese in P0 | User decision is recorded; A25 still needs catalog and runtime evidence |
| G04 Release environments | Open | macOS release matrix, actual Linux session, and native behavior checks |

An open gate does not become a product defect merely because implementation has not started, and a document review cannot close it. Any future scope change must update the [requirements](../requirements.md), [architecture](../architecture.md), and [acceptance mapping](../traceability.md).

## Document checks

Completed for this revision: English prose and labels reviewed; local Markdown links and code-fence balance checked; all original external references retained; A01 through A25 appear once in the requirements and once in the acceptance mapping; git diff --check passes. These checks do not exercise the application or close provider gates.
