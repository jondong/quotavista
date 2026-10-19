# Persistence and recovery

Follow [local persistence](../../../docs/architecture.md#local-persistence) and [credential lifecycle](../../../docs/architecture.md#credential-lifecycle-and-crash-recovery).

SQLite is backend-owned, in the OS application-data directory. Use migrations, WAL, a serialized writer, and foreign-key/busy-timeout settings on every connection. Pin a SQLite release with the documented WAL fixes and verify the linked runtime version during setup.

Tokens use canonical integer text with u64 facts and checked u128 totals. Money uses canonical decimal text with explicit currency and checked precision. Never aggregate accounting values through SQLite REAL or JavaScript Number.

Keep snapshots, usage facts, coverage, checkpoints, candidate sync runs, and published partitions distinct. Publish a replacement partition atomically only after completeness is established; a partial candidate must not erase or add to its published predecessor.

Check generation and lifecycle inside the write transaction while holding the per-connection mutation fence. Recovery precedes scheduling. External durable journals contain opaque references only; native secret values never enter SQLite, backups, journals, or fixtures. Deletion must fence late results and purge containing managed backups before acknowledgement.

Phase 1 establishes and fault-tests these boundaries with synthetic data. Full provider synchronization, 90-day retention, and managed restore flows are later-phase work.

## P1-03 adapter contract

`quotavista_core::storage::Repository` is a backend-only async port. `quotavista_infrastructure::sqlite::SqliteRepository::open` requires an explicit path; it is not yet called by the desktop. Until startup recovery and single-instance coordination exist, use it only with throwaway synthetic stores.

- SQLx is pinned to `0.9.0`; `libsqlite3-sys = 0.37.0` bundles SQLite `3.51.3`. Startup and tests assert the linked version (not a system `sqlite3` executable). This version includes the WAL-reset fix; do not enable SQLx's unbundled SQLite feature.
- One writer connection takes `BEGIN IMMEDIATE` before checking generation/lifecycle/scope. Three read-only pooled connections use explicit read transactions for published headers/facts. Each connection sets WAL, foreign keys, a 5-second busy timeout, and FULL synchronous mode. There is no unbounded retry loop.
- All amounts are canonical TEXT. Snapshot token totals support `u128`; fact tokens are bounded to `u64`. Decimal/currency parsing reuses core types. Never use SQLite `SUM(amount)` for accounting.
- `stage_page` compares the previous cursor and commits its page/upserts and next cursor together (1,000 facts/page, 100,000 distinct facts/candidate). `complete_candidate` is an internal caller attestation of complete, settled, untruncated authoritative coverage, not a provider pagination algorithm.
- Publication checks the candidate's base revision and current generation/lifecycle under the writer lock. It deletes/replaces only that partition's facts and commits the pointer, coverage, checkpoint, success time, revision, and staging cleanup together. No incomplete or overlapping source partitions may publish. A repeated published run is a no-op.
- Within one candidate, the last accepted cursor-ordered page wins for a duplicate identity. Stale page retries fail cursor compare-and-swap; opaque `source_revision` strings are retained but not sorted. A provider adapter must resolve conflicting source revisions before attesting the candidate as authoritative; that provider-specific ordering is not implemented here.
- Event identity is scope/dataset/event-id/metric, independent of timestamp and source revision. A correction within a replacement updates one fact. A correction that would move an event across partitions fails closed; multi-partition reconciliation is deferred. Bucket identity includes partition context and half-open bounds.
- Only the initial schema is supported. Reopening validates migration checksums; a pending migration against a populated store is refused until a consistent backup/maintenance coordinator is implemented. Never substitute a copy of a live WAL database. Metadata tables for lifecycle, alerts, and managed backups do not implement those later workflows.

Run `cargo test --locked -p quotavista-infrastructure` for temporary-file migration/reopen, exact-number, rollback, publication, stale-generation, pool-setting, and external-lock fixtures. These tests never resolve or read an application data directory.
