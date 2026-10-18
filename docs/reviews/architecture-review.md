# Architecture Review

Date: 2026-10-18

Reviewed versions: Architecture 0.4 and Requirements 0.9

Method: Internal design review by the authoring assistant after requirements refinement, followed by a separate failure-path and cross-document consistency pass. No independent reviewer, executable prototype, or security certification is implied.

## Verdict

The revised architecture is ready for bounded prototype implementation. No unresolved documentation-level correctness blocker remains in the reviewed scope. Provider authorization, token/history capabilities, native behavior, and performance are unvalidated and remain implementation or release gates.

This review keeps Tauri 2, React/TypeScript, Rust, and SQLite as the design baseline. It adds explicit contracts and recovery rules without introducing a cloud backend, inference proxy, or background system service. English and Simplified Chinese are required P0 interface locales; repository documentation remains English.

## Review criteria

- Every P0 requirement maps to a component and expected evidence.
- Provider-specific fields stay out of domain and UI calculations.
- Identity, units, time boundaries, provenance, and completeness survive serialization.
- Writes, revisions, cancellation, and lifecycle changes have explicit transaction boundaries.
- Credentials and the database recover without assuming a shared transaction.
- Runtime limits are bounded and failure never becomes fake zero or completeness.
- Platform and bilingual behavior have observable verification gates.

## Findings and disposition

Severity describes the risk of leaving the earlier design unchanged. Resolved denotes a specification correction, not a proven implementation fix.

| ID | Severity | Finding | Resolution |
| --- | --- | --- | --- |
| AR01 | High | A generation check before a write allowed a lifecycle mutation between the check and commit | Check lifecycle and generation inside the write transaction; serialize mutations and hold a recovered lifecycle fence through commit |
| AR02 | High | Secret-first connection creation could leave an orphan credential after a crash or backup restore | Journal candidate and superseded references before secret writes, including an external nonsecret journal that survives managed database restoration |
| AR03 | High | Credential replacement could preserve history for an unrelated provider identity | Prove matching account/workspace scope or create a separate explicitly selected replacement; enforce one active P0 connection per provider |
| AR04 | High | Historical upserts could not remove revised records and page failures could publish incomplete replacements | Stage a candidate epoch, advance its cursor with its rows, and atomically publish only a complete authoritative partition |
| AR05 | High | Source events and buckets could overlap; UTC buckets could be misrepresented as local-day data | Select one authority per metric interval and return native lanes, actual boundaries, exclusions, and provisional coverage |
| AR06 | High | Managed backups could restore a deleted connection or lose the cleanup operation itself | Maintain an external deletion ledger, fence scheduling, purge containing backups, and reconcile journals before managed restore |
| AR07 | Medium | Commands, errors, partial metrics, and UI invalidation semantics were too abstract for consistent implementation | Add command/result contracts, typed errors, config revisions, metric provenance, and revision-hint events |
| AR08 | Medium | Polling, oversized responses, repeated cursors, long retries, and query work lacked bounds | Set explicit time, byte, page, record, concurrency, and cancellation limits; cooldowns survive restart and manual refresh respects them |
| AR09 | Medium | Alert first samples, severity conflicts, configuration edits, and lifecycle races lacked episode rules | Persist episodes, prioritize exhaustion, preserve deduplication, and claim dispatch against current lifecycle state |
| AR10 | Medium | Global audience and display localization were not reflected in transport and notification design | Bundle en/zh-CN catalogs, keep locale separate from zone/currency, persist selection, and localize templates without reissuing alerts |
| AR11 | Medium | Numeric representation did not define supported integer and decimal limits or alias length units | Specify u64 facts, checked u128 totals, bounded exact decimals, canonical text storage, Unicode alias length, and explicit overflow errors |

The final consistency pass tightened three boundaries: the live-query deadline includes retries and backoff, long blocking work uses cooperative cancellation, and external journals require durable replacement before secret mutation. It also added explicit locked/unavailable store errors and an alert-eligibility field separate from collection freshness. These clarify AR02, AR07, AR08, and AR09.

## Recovery walkthroughs

These are design-level walkthroughs, not executed tests.

| Interruption or failure | Required recovery |
| --- | --- |
| Crash after candidate secret write, before activation | External journal identifies the candidate; remove it or resume the validated operation before scheduling |
| Crash after rotation commits, before old secret cleanup | Keep the committed new reference and retry deleting the superseded reference |
| Database restore predates credential rotation | Reconcile the external journal; do not lose track of new or superseded references or pretend an absent old key is valid |
| Page three fails during a replacement | Retain the published partition; preserve candidate cursor and rows together; report pending/failed revision rather than current completeness |
| A complete revised partition omits an old event | Retire its old contribution once, only within that authoritative scope |
| User deletes a connection while HTTP is returning | The lifecycle fence and transaction guard reject the old result without changing coverage, timestamps, or alerts |
| External deletion ledger persists but the database write fails | Recover Deletion pending from the ledger and block collection until cleanup is reconciled |
| Keychain or Secret Service is locked during removal | Retain deletion intent, stop collection, and expose retry instead of reporting success |
| Disk fills during publication or cursor commit | Roll back the entire affected transaction and retain the last committed state |
| Automatic refresh is disabled and the app wakes | Read cache; do not contact the provider without an explicit user action |
| User switches UI language during a statistics request | Keep domain range/units unchanged; ignore obsolete filter responses; reformat display and future notifications without another alert episode |

## Traceability and evidence

The [acceptance mapping](../traceability.md) covers all 25 criteria. Command and storage descriptions are design contracts, not claims that code exists. Prototype validation must exercise the invariants above with deterministic clocks, fixture adapters, store failures, and transaction interruption.

| Gate | Status and required next evidence |
| --- | --- |
| G01 | Open: Kimi authorization and shared-pool retrieval still require real-account reconciliation |
| G02 | Open: provider tokens, granularity, current-day visibility, lookback, source revisions, and permissions need capability reports |
| G03 | Product choice closed by user: ship en and zh-CN. A25 needs catalog and runtime validation |
| G04 | Open: Ubuntu 26.04 GNOME x86_64 session and native behavior, plus a macOS release matrix, must be recorded |

No native build, data reconciliation, benchmark, credential-store operation, or failure-injection test has run. Documentation checks establish only internal consistency, links, identifiers, and formatting.

## Document checks

Completed for this revision: local links and balanced code fences checked; both English Mermaid diagrams retained; original technical references preserved; requirements and architecture version references checked across review records and the development plan; all 25 acceptance criteria mapped to components and future evidence; git diff --check passes.
