# Errors and bounded work

Use the stable error taxonomy and limits in [Architecture](../../../docs/architecture.md#bounded-work-and-error-handling). Keep unavailable capabilities separate from failed queries and unknown values separate from numerical zero.

Validate aliases, secrets, enum values, revisions, date ranges, and reporting zones before side effects. Preserve secret contents; reject header line breaks and inputs over 64 KiB. Never echo rejected values.

Use checked integer/decimal arithmetic. Range calculations use IANA calendar boundaries, Monday weeks, half-open intervals, and one captured as-of instant. Preserve UTC-only source boundaries and report excluded scopes.

Preserve last committed data on query failures. Failed storage writes cannot advance successful synchronization time. Cancellation must release slots and stop bounded database/CPU batches. Phase 1 exercises these outcomes through deterministic synthetic failures; actual provider retry and pagination policy arrives in later phases.

P1-02's `SafeError` in `crates/contracts/src/lib.rs` derives its private code/message/retryability combination through `new`; getters are read-only, and deserialization rejects mismatched message or retryability. Domain-to-wire invariant failures in `crates/contracts/src/mapping.rs` are `internal_error`, never caller validation or provider invalid-response errors. Keep metric unit/value fields private and validate canonical values by unit on every construction/decode path: tokens can reach u128::MAX and must not pass through the narrower Decimal type. Core `QueryKind` makes current versus historical/delayed/cache classification explicit; only validated current, nonexpired, settled facts with source-time evidence can be alert candidates. `sync_pending` alone does not erase eligibility of a previously verified fact.
