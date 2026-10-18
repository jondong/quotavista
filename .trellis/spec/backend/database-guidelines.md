# Persistence and recovery

Follow [local persistence](../../../docs/architecture.md#local-persistence) and [credential lifecycle](../../../docs/architecture.md#credential-lifecycle-and-crash-recovery).

SQLite is backend-owned, in the OS application-data directory. Use migrations, WAL, a serialized writer, and foreign-key/busy-timeout settings on every connection. Pin a SQLite release with the documented WAL fixes and verify the linked runtime version during setup.

Tokens use canonical integer text with u64 facts and checked u128 totals. Money uses canonical decimal text with explicit currency and checked precision. Never aggregate accounting values through SQLite REAL or JavaScript Number.

Keep snapshots, usage facts, coverage, checkpoints, candidate sync runs, and published partitions distinct. Publish a replacement partition atomically only after completeness is established; a partial candidate must not erase or add to its published predecessor.

Check generation and lifecycle inside the write transaction while holding the per-connection mutation fence. Recovery precedes scheduling. External durable journals contain opaque references only; native secret values never enter SQLite, backups, journals, or fixtures. Deletion must fence late results and purge containing managed backups before acknowledgement.

Phase 1 establishes and fault-tests these boundaries with synthetic data. Full provider synchronization, 90-day retention, and managed restore flows are later-phase work.
