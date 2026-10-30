# Boundaries and contracts

The layout and command table are defined in [Architecture](../../../docs/architecture.md#modules-and-dependencies). Since P1-01 the workspace directories below exist as scaffold crates; their internals grow with later tasks.

- `crates/core` owns domain rules and application ports. It must not depend on Tauri, provider implementations, SQLx, or native credential stores.
- `crates/providers` implements provider ports without UI or database writes. Synthetic adapters serve demo mode; Phase 2 adds live OpenRouter/Kimi adapters that never issue a request without a resolved credential.
- `crates/infrastructure` implements storage, secret, and network boundaries. Since P2-04 it also owns provider selection for refresh (`refresh::router::ProviderRouter`): a connection whose stored provider id starts with `synthetic` always routes to the simulated adapters (Kimi side chosen by the `kimi` substring, mirroring `SimulatedRouter::route`), while `kimi`/`openrouter` route to the live adapters built per call from the transport plus the credential resolved via `credential_ref`. Any credential-resolution failure yields a `None` token so the adapter itself fails `PermissionRequired` before any request; an unknown provider id fails typed (`InvalidResponse`) without touching the network. Secret reads are blocking and must be offloaded with `spawn_blocking`.
- `apps/desktop/src-tauri` assembles dependencies and exposes explicit commands. It is the composition root.

- `crates/contracts` owns serde DTOs and the `ts-rs` TypeScript generator. It depends on core, never the reverse; it does not register Tauri handlers.
Keep domain types independent of transport serialization. Mechanically generate or check TypeScript DTOs from Rust transport types. Return versioned envelopes, safe typed errors, configuration revisions, and revision-only change events. Do not expose arbitrary URLs, SQL, shell operations, or filesystem paths through IPC.

See [requirements](../../../docs/requirements.md#connection-lifecycle-and-user-flows) for separate lifecycle, query-health, freshness, and coverage states. Do not collapse them into one connected flag.
