# Acceptance Traceability

Date: 2026-10-18

Baselines: Requirements 0.9 and Architecture 0.4

This mapping connects required outcomes to planned components and evidence. Every implementation status is Pending because the repository has no runnable application. A document review is not a substitute for the evidence listed here.

Phases refer to the [development plan](development-plan.md). Requirements and design review findings are recorded in the [requirements review](reviews/requirements-review.md) and [architecture review](reviews/architecture-review.md).

| Criterion | Planned component or design section | Essential scenario or evidence | Phase | Status |
| --- | --- | --- | --- | --- |
| A01 | Provider adapters and capability reports | Real Kimi shared pool and OpenRouter account balance; compare native units and timestamps, not demo data | 2 | Pending; G01/G02 |
| A02 | Scope and product identity | Web/app/Code shared consumption counted once; independent limits cannot substitute | 2 | Pending |
| A03 | Kimi adapter validation | Compare the same period and scope with the official member page, recording allowed display precision and delay | 2 | Pending; G01 |
| A04 | Metric scope and permission state | Regular-key budget does not become an account balance; account/key overlap not summed | 2 | Pending |
| A05 | Metric transport and UI states | Real zero, null, unsupported, stale, and failed states remain distinct | 1 | Pending |
| A06 | Scheduler, typed errors, and dataset commits | Offline, timeout, 429/cooldown, permission failure, and disk-write failure preserve valid data and isolate connections | 2, 4 | Pending |
| A07 | Per-metric freshness and lifecycle | Elapsed-age staleness, cache on restart, source delay, wake, and manual-only mode | 1, 2, 4 | Pending |
| A08 | Persisted alert episodes | First fresh low value, repeated poll, recovery, exhaustion precedence, restart, configuration change, and denied notifications | 4 | Pending |
| A09 | SQLite and lifecycle recovery | Restart restores original value times; partial migration and incomplete journal steps do not appear successful | 1, 4 | Pending |
| A10 | SecretStore, IPC, logs, and fixtures | Inspect persisted files and sanitized errors; inject store/DB failures without disclosing credentials | 1, 2, 4 | Pending |
| A11 | Native desktop integration | Install and exercise core flows on supported macOS and Ubuntu 26.04 GNOME x86_64; record session type and missing tray behavior | 1, 5 | Pending; G04 |
| A12 | Local composition root | All P0 flows work without GitHub or a QuotaVista cloud login | 1, 5 | Pending |
| A13 | Range resolver and aggregation | Inclusive UI dates, half-open instants, Monday weeks, DST, UTC-only source lanes, and as-of clipping | 3 | Pending |
| A14 | Domain numeric semantics | Cached/reasoning inclusion rules, u64 facts, checked totals, incompatible metrics, and an all-missing token result | 3 | Pending; G02 |
| A15 | Statistics transport and charts | Native Kimi percentage-point consumption alongside OpenRouter tokens/charges with explicit exclusions | 3 | Pending; G02 |
| A16 | Canonical fact identity and source authority | Repeat pages, event timestamp correction, resets, top-ups, and overlapping event/bucket sources | 3 | Pending |
| A17 | Coverage intervals and provisional state | Empty complete interval versus missing history; partial pages, truncation, unsettled today, and source retention | 3 | Pending; G02 |
| A18 | Connection coordinator and operation journals | Failed rotation preserves old key; mismatched identity creates a new scope; late results cannot recreate disabled/deleted connections | 2, 4 | Pending |
| A19 | Candidate staging and atomic publication | A complete revised response removes prior facts; an incomplete candidate cannot replace a verified partition | 3 | Pending |
| A20 | Retention, backup manifest, and deletion ledger | Cutoff intersects a bucket; cleanup or restore interrupted; containing backups purged before removal acknowledgement | 1, 3, 4 | Pending |
| A21 | Native secret stores and restricted IPC | Session-only restart, locked store, untrusted caller, missing capability, and privileged command validation | 1, 2, 4 | Pending |
| A22 | Input validators and bounded I/O | Invalid aliases, dates, zones, precision, header line breaks, repeated cursor, oversized decompressed body, and work limits | 1, 2, 3 | Pending |
| A23 | Async workers, writer queue, and benchmarks | UI navigation during slow work; 100,000-fact local query benchmark; disk-full and migration interruption | 1, 3, 5 | Pending |
| A24 | Repository documentation policy | English documentation and diagram labels, valid links, consistent version references, and patch-format checks | Documentation and 5 | Pending release audit |
| A25 | Bundled locale catalogs and UI settings | en/zh-CN key parity, initial selection, immediate switch, persisted preference, fallback, accessible labels, formatted dates/numbers, notification templates | 1, 4, 5 | Pending runtime evidence |

## Release evidence rules

- A01 and A03 require real-account reconciliation; fixtures alone cannot pass them.
- Synthetic data is appropriate for numerical boundaries, failure injection, revision logic, and alert timing. Describe it as synthetic evidence.
- Provider-specific capabilities remain gated until measured; explicit Unsupported is honest behavior, but it does not automatically satisfy a required unavailable core metric.
- Bilingual key parity is necessary for A25, but both languages still need runtime interaction checks.
- English documentation can pass its current document check while A24's final release audit remains pending.
- Each future verification entry should record criterion, build or commit, environment, method, result, and any gap. Do not replace Pending with Passed based on a design walkthrough.
