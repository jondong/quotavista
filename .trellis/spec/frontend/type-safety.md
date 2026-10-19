# Types and numeric values

Use strict TypeScript and mechanically generated or checked Rust transport contracts. See [application contracts](../../../docs/architecture.md#application-contracts). Do not maintain parallel DTO definitions by hand or silence boundary mismatches with unchecked casts.

Use discriminated states for unavailable, missing, partial, and current values. Preserve contract_version, request_id, revisions, nullable values, and safe error codes through the IPC client.

Tokens and exact decimals cross IPC as strings. Never convert large token counts or monetary totals through Number for accounting. Use locale-aware display formatting without changing the underlying value, currency, time zone, or coverage. Select chart scaling deliberately and keep exact labels/totals available.

P1-02 owns the wire source in `crates/contracts/src/lib.rs` and explicit domain mappings in `crates/contracts/src/mapping.rs`. Regenerate `apps/desktop/src/generated/contracts.ts` with `cargo run --locked -p quotavista-contracts --bin generate-contracts`; CI runs the same command with `-- --check`. Do not edit the generated file. The request-only secret input is not part of any response, event, or debug representation. Command registration and UI decoding remain P1-07 work.

Generated types describe canonical wire objects: every nullable field is emitted explicitly as `null`. For optional Rust fields (including `SettingsPatch`), deserialization also accepts omission as `None`; omission and `null` both mean “leave unchanged” in a settings patch. Canonical TypeScript senders include all patch fields, using `null` for untouched values. `apps/desktop/src/contracts.typecheck.ts` compiler-checks all eleven request shapes, canonical empty patches, and rejected numeric/provider shapes; Rust integration tests independently check decoding and canonical serialization. Generated declarations are not runtime validators.

Overview requests use `OverviewRangeInput` without a grain; only statistics `RangeInput` requires one. Source metadata (`SourceLabel`) is a validated nonempty, control-free string of at most 128 Unicode code points, not a guarantee of HTML safety. SafeError code/message/retryability are a sealed combination; do not synthesize arbitrary prose for known codes. Metric unit/value invariants are checked in Rust (including u128 token totals); the generated `string` aliases cannot express those bounds.

Every metric now requires a `query_kind` (`validated_current`, `unvalidated`, `historical`, `delayed_historical`, or `stale_upstream_cache`). Rust derives `alert_eligible` from that classification and the sealed metric state, and checked decoding rejects inconsistent claims. Generated TypeScript describes the fields but cannot express their cross-field invariants; do not infer live-current status from a recent `observed_at` or trust an arbitrary boolean as proof of freshness. Rust accessors expose the read-only state and SafeError retry timestamp.
