# Types and numeric values

Use strict TypeScript and mechanically generated or checked Rust transport contracts. See [application contracts](../../../docs/architecture.md#application-contracts). Do not maintain parallel DTO definitions by hand or silence boundary mismatches with unchecked casts.

Use discriminated states for unavailable, missing, partial, and current values. Preserve contract_version, request_id, revisions, nullable values, and safe error codes through the IPC client.

Tokens and exact decimals cross IPC as strings. Never convert large token counts or monetary totals through Number for accounting. Use locale-aware display formatting without changing the underlying value, currency, time zone, or coverage. Select chart scaling deliberately and keep exact labels/totals available.
