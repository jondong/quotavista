# Boundaries and contracts

The layout and command table are defined in [Architecture](../../../docs/architecture.md#modules-and-dependencies). Since P1-01 the workspace directories below exist as scaffold crates; their internals grow with later tasks.

- `crates/core` owns domain rules and application ports. It must not depend on Tauri, provider implementations, SQLx, or native credential stores.
- `crates/providers` implements provider ports without UI or database writes. Phase 1 uses synthetic adapters only.
- `crates/infrastructure` implements storage, secret, and network boundaries.
- `apps/desktop/src-tauri` assembles dependencies and exposes explicit commands. It is the composition root.

- `crates/contracts` owns serde DTOs and the `ts-rs` TypeScript generator. It depends on core, never the reverse; it does not register Tauri handlers.
Keep domain types independent of transport serialization. Mechanically generate or check TypeScript DTOs from Rust transport types. Return versioned envelopes, safe typed errors, configuration revisions, and revision-only change events. Do not expose arbitrary URLs, SQL, shell operations, or filesystem paths through IPC.

See [requirements](../../../docs/requirements.md#connection-lifecycle-and-user-flows) for separate lifecycle, query-health, freshness, and coverage states. Do not collapse them into one connected flag.
