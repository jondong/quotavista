# Structure and IPC

The frontend lives in `apps/desktop/src` (scaffolded in P1-01 with a single synthetic-status view), with Overview, Statistics, Connections, and Settings planned. See [Architecture](../../../docs/architecture.md#planned-source-layout). Choose internal component folders as screens land and document real examples afterward.

All provider access, accounting, persistence, and secret-store operations belong to Rust. Use a narrow typed IPC client and bundled assets. Never fetch provider endpoints from the renderer or persist credentials in browser storage.

Phase 1 must expose synthetic provenance on every demo account and retain an application-level demo indicator. A simulated validation result is not evidence of a real connection.
