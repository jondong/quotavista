# Native secret store crates (checked 2026-10, official crates.io releases)

Chosen per architecture 0.4: keyring-core with explicit per-platform stores, no plaintext fallback.

## Pinned versions

| Crate | Version | Rust MSRV | Role |
|---|---|---|---|
| keyring-core | =1.0.1 | 1.85 | Port types: `Entry`, `Error`, `CredentialStoreApi` |
| apple-native-keyring-store | =1.0.2 | 1.85 | macOS Keychain store (`keychain::Store`) |
| dbus-secret-service-keyring-store | =1.0.1 | 1.88 | Linux Secret Service store (`Store`) |

Workspace toolchain is 1.99.1 (rust-toolchain.toml, CI), so MSRVs are satisfied.

## API facts used

- `keyring_core::api::CredentialStoreApi::build(&self, service, user, modifiers) -> Result<Entry>`;
  `pub type CredentialStore = dyn CredentialStoreApi + Send + Sync`.
- `apple_native_keyring_store::keychain::Store::new() -> Result<Arc<Store>>` (User/login keychain domain).
- `dbus_secret_service_keyring_store::Store::new() -> Result<Arc<Store>>`; construction connects to the
  session bus and fails on headless hosts → map to `Unavailable`.
- `Entry::set_secret(&[u8])`, `Entry::get_secret() -> Result<Vec<u8>>`,
  `Entry::delete_credential() -> Result<()>`. Entries are `Send + Sync`.
- `keyring_core::Error` mapping:
  - `NoStorageAccess(..)` → store locked (access rules deny; e.g. locked keychain)
  - `NoEntry` → no credential for the reference (read → needs credentials; delete → Ok, idempotent)
  - `TooLong(..) | Invalid(..) | BadEncoding(..)` → validation
  - `PlatformFailure | BadStoreFormat | BadDataFormat(..) | NoDefaultStore | NotSupportedByStore(..) | Ambiguous(..)` → unavailable
- Error `Display` text may embed platform details but never the secret value; our error enum stays
  unit variants so no platform detail or value can leak upward.

## Blocking-call isolation

Both stores perform blocking platform I/O (Security framework daemon, D-Bus). The P1-04
`SecretReferenceStore` port is synchronous, so the adapter owns a dedicated worker thread with a
command channel; all keyring calls run on that thread, serialized, off any async executor thread.

## References and validation

- Opaque versioned references: `qv1:<backend-tag>:<opaque>` where backend-tag is
  `macos-keychain` | `linux-secret-service` | `session-only`. Scope match: a store only accepts
  references carrying its own tag.
- Synthetic identity validator (Phase 1, cannot prove provider access): secret must be non-empty,
  ≤ 64 KiB, contain no CR/LF (error-handling spec header rule); rejected values are never echoed.
- macOS smoke checks: service name `quotavista-test-p1-05-<pid>`, disposable synthetic values,
  every created entry deleted afterwards; ignored by default (`cargo test -- --ignored`), so CI
  never touches a real store. Linux native evidence stays pending on this macOS host.

## Sources

- crates.io + vendored sources of the three pinned releases (inspected under /tmp, not committed).
