# P1-03 design

## Approach and boundaries

SQLx and SQLite implement core storage ports. Use temporary databases for tests and the OS data directory at runtime. Keep the published revision pointer and partition replacement transactional. Persist schema needed for later features without claiming complete retention, backfill, alerts, or managed recovery.

## Dependencies and contracts

Use the baseline linked in [PRD](prd.md). Predecessors: [P1-02](../archive/2026-10/10-18-p1-02-contracts/prd.md) Update affected DTO mappings, migrations, and consumers together; preserve core dependency direction and synthetic provenance.

## Compatibility and rollback

Preserve en/zh-CN behavior and independent reporting time zone/currency. Use temporary synthetic state during development. Keep changes reviewable in one task branch; revert code as a unit if validation fails. Never delete a real application data directory as a rollback shortcut. Any persisted-schema change requires a tested migration/recovery path before activation.

## Deferred work

No provider pagination/backfill, full 90-day retention, managed backup/restore implementation, or complete usage analytics.

## P1-03 implementation decisions

- Core owns `storage::Repository` and backend-only typed records; infrastructure owns `sqlite::SqliteRepository` and `migrations/0001_persistence.sql`. Existing core numeric, lifecycle, provenance, and interval types are reused. No IPC contract or frontend consumer changes are needed until desktop composition.
- SQLx `0.9.0` with bundled `libsqlite3-sys 0.37.0` links SQLite `3.51.3`. Verify `SELECT sqlite_version()` at open and in the pool fixtures. The selected version contains the upstream WAL-reset fix; the system SQLite executable is not the runtime under test. See [SQLite WAL guidance](https://sqlite.org/wal.html) and [SQLx SQLite options](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html).
- One writer pool member plus `BEGIN IMMEDIATE` serializes mutation checks and commit, including across independent adapter instances. Read-only query connections are bounded to three; published facts and metadata are read in one transaction. Each connection sets WAL, foreign keys, FULL synchronous mode, and a five-second busy timeout. Busy failures return a safe storage error without automatic retries.
- Settings use compare-and-swap revisions. Snapshot amount, source/observation/success times, and data revision commit together. Canonical exact values use TEXT; no accounting calculation uses SQL floating point.
- Candidate page/cursor commits are compare-and-swap transactions. Publication requires an explicitly completed authoritative candidate, the original published base revision, an active matching generation, and no pending lifecycle operation. Complete replacement removes missing facts and updates published state, coverage, checkpoint, success time, and revision atomically; staging is removed only on success.
- Event identity excludes corrected occurrence time and source revision. Duplicate pages upsert one event; same-partition corrections replace it. A cross-partition event move or overlapping authority/grain fails closed instead of double counting. Coordinated multi-partition correction remains later provider work.
- Runtime startup is deliberately not connected to this adapter: single-instance protection, external lifecycle/deletion fences, secret recovery, and OS path resolution belong to subsequent tasks. Tests open only `tempfile::TempDir` paths.
- There is no released predecessor schema. Fresh migrations are transactional and reopen validates the applied migration. Pending migrations on a populated store are rejected before running them, preserving the store until the consistent-backup/maintenance coordinator exists. This is not a managed migration/restore implementation.

Reproduction and criterion-level evidence are recorded in [validation.md](validation.md). Release acceptance remains Pending.
