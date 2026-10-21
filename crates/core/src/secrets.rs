//! Secret store port: value-carrying operations behind opaque versioned
//! references.
//!
//! Journals, the database, logs, events, and DTOs carry only opaque
//! references; secret values enter through [`SecretStore::stage_candidate`],
//! stay in memory until [`SecretReferenceStore::write_candidate`] commits them
//! to the backend, and never appear in errors. [`SessionSecrets`] is the
//! explicit opt-in memory-only backend: its values never leave process memory,
//! so a restart finds no retrievable secret and the connection must enter
//! needs-credentials.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::journal::{SecretReferenceStore, SecretStoreError};

/// Reference scheme prefix, versioned so a future scheme can never be confused
/// with this one.
const SCHEME: &str = "qv1";
/// Longest accepted secret value, per the backend error-handling spec.
pub const MAX_SECRET_BYTES: usize = 64 * 1024;
/// Longest accepted opaque reference.
const MAX_REFERENCE_BYTES: usize = 256;

/// Which backend holds a secret. The tag is part of every opaque reference so
/// a store only ever touches its own entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretBackend {
    MacosKeychain,
    LinuxSecretService,
    /// Explicit opt-in memory-only storage; nothing reaches disk.
    SessionOnly,
}

impl SecretBackend {
    pub fn tag(self) -> &'static str {
        match self {
            Self::MacosKeychain => "macos-keychain",
            Self::LinuxSecretService => "linux-secret-service",
            Self::SessionOnly => "session-only",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "macos-keychain" => Some(Self::MacosKeychain),
            "linux-secret-service" => Some(Self::LinuxSecretService),
            "session-only" => Some(Self::SessionOnly),
            _ => None,
        }
    }
}

/// Why a value-carrying secret operation failed. Unit variants only: a store
/// error must never carry platform detail or any part of a secret value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretError {
    /// The store is locked. The action stays visibly pending; never downgrade
    /// to session-only or plaintext storage.
    Locked,
    /// The store cannot be reached at all (no keychain daemon, no session
    /// bus). Distinct from [`SecretError::Locked`].
    Unavailable,
    /// No retrievable secret exists for the reference; the user must supply
    /// credentials again. A session-only secret after a restart lands here.
    NeedsCredentials,
    /// The reference or value failed synthetic validation.
    Validation,
}

/// Value-carrying secret store port. Every store is also a
/// [`SecretReferenceStore`], so the lifecycle coordinator can journal
/// candidate writes and cleanups without ever seeing a value.
pub trait SecretStore: SecretReferenceStore {
    /// Backend this store talks to; references for other backends are refused.
    fn backend(&self) -> SecretBackend;
    /// Validate a candidate value and hold it in memory under its reference
    /// until [`SecretReferenceStore::write_candidate`] commits it to the
    /// backend. Staging alone has no durable side effect, so a crash between
    /// staging and commit leaves nothing behind.
    fn stage_candidate(&self, reference: &str, secret: &str) -> Result<(), SecretError>;
    /// Read a committed value. [`SecretError::NeedsCredentials`] means no
    /// retrievable secret exists for the reference.
    fn read(&self, reference: &str) -> Result<String, SecretError>;
}

/// Mint an opaque versioned reference for a backend. `opaque` identifies the
/// entry inside the backend and must carry no secret material itself.
pub fn mint_reference(backend: SecretBackend, opaque: &str) -> String {
    format!("{SCHEME}:{}:{opaque}", backend.tag())
}

/// Backend a reference belongs to, when the reference is well-formed.
pub fn reference_backend(reference: &str) -> Option<SecretBackend> {
    if reference.len() > MAX_REFERENCE_BYTES
        || !reference.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return None;
    }
    let rest = reference.strip_prefix("qv1:")?;
    let (tag, opaque) = rest.split_once(':')?;
    if opaque.is_empty() {
        return None;
    }
    SecretBackend::from_tag(tag)
}

/// Validate an opaque reference syntactically.
pub fn validate_reference(reference: &str) -> Result<(), SecretError> {
    reference_backend(reference)
        .map(|_| ())
        .ok_or(SecretError::Validation)
}

/// Synthetic identity validator, Phase 1: shape only; it cannot prove provider
/// access. Rejects empty or oversized values and header line breaks, per the
/// backend error-handling spec. The rejected value is never echoed.
pub fn validate_secret(secret: &str) -> Result<(), SecretError> {
    if secret.is_empty() || secret.len() > MAX_SECRET_BYTES || secret.contains(['\r', '\n']) {
        return Err(SecretError::Validation);
    }
    Ok(())
}

impl<T: SecretReferenceStore + ?Sized> SecretReferenceStore for std::sync::Arc<T> {
    fn write_candidate(&self, reference: &str) -> Result<(), SecretStoreError> {
        (**self).write_candidate(reference)
    }

    fn delete_reference(&self, reference: &str) -> Result<(), SecretStoreError> {
        (**self).delete_reference(reference)
    }
}

impl<T: SecretStore + ?Sized> SecretStore for std::sync::Arc<T> {
    fn backend(&self) -> SecretBackend {
        (**self).backend()
    }

    fn stage_candidate(&self, reference: &str, secret: &str) -> Result<(), SecretError> {
        (**self).stage_candidate(reference, secret)
    }

    fn read(&self, reference: &str) -> Result<String, SecretError> {
        (**self).read(reference)
    }
}

/// Explicit opt-in memory-only store. Values live in process memory and are
/// lost at exit; nothing is written to disk, so after a restart every read
/// reports [`SecretError::NeedsCredentials`]. Selecting this store is a
/// deliberate user choice, never a fallback for a locked native store.
#[derive(Debug, Default)]
pub struct SessionSecrets {
    state: Mutex<SessionState>,
}

#[derive(Debug, Default)]
struct SessionState {
    staged: BTreeMap<String, String>,
    live: BTreeMap<String, String>,
}

impl SessionSecrets {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many staged candidates the store still holds. Staged candidates
    /// exist only between `stage_candidate` and
    /// `write_candidate`/`delete_reference`; a refused flow that never
    /// transfers ownership to the journal must leave none behind. Test
    /// inspection hook.
    pub fn staged_count(&self) -> usize {
        self.state.lock().unwrap().staged.len()
    }
}

impl SecretStore for SessionSecrets {
    fn backend(&self) -> SecretBackend {
        SecretBackend::SessionOnly
    }

    fn stage_candidate(&self, reference: &str, secret: &str) -> Result<(), SecretError> {
        if reference_backend(reference) != Some(SecretBackend::SessionOnly) {
            return Err(SecretError::Validation);
        }
        validate_secret(secret)?;
        self.state
            .lock()
            .unwrap()
            .staged
            .insert(reference.to_owned(), secret.to_owned());
        Ok(())
    }

    fn read(&self, reference: &str) -> Result<String, SecretError> {
        if reference_backend(reference) != Some(SecretBackend::SessionOnly) {
            return Err(SecretError::Validation);
        }
        self.state
            .lock()
            .unwrap()
            .live
            .get(reference)
            .cloned()
            .ok_or(SecretError::NeedsCredentials)
    }
}

impl SecretReferenceStore for SessionSecrets {
    /// Commit the staged value. A reference that was never staged means the
    /// caller skipped the value-carrying step, which the reference-only port
    /// cannot express better than `Unavailable`.
    fn write_candidate(&self, reference: &str) -> Result<(), SecretStoreError> {
        let mut state = self.state.lock().unwrap();
        match state.staged.remove(reference) {
            Some(value) => {
                state.live.insert(reference.to_owned(), value);
                Ok(())
            }
            None => Err(SecretStoreError::Unavailable),
        }
    }

    /// Deleting an absent reference succeeds, so recovery resumes idempotently.
    fn delete_reference(&self, reference: &str) -> Result<(), SecretStoreError> {
        let mut state = self.state.lock().unwrap();
        state.staged.remove(reference);
        state.live.remove(reference);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE: &str = "qv1:session-only:synthetic-candidate";
    const SECRET: &str = "synthetic-secret-value";

    #[test]
    fn reference_carries_its_backend_tag() {
        for backend in [
            SecretBackend::MacosKeychain,
            SecretBackend::LinuxSecretService,
            SecretBackend::SessionOnly,
        ] {
            let reference = mint_reference(backend, "synthetic-entry");
            assert_eq!(reference_backend(&reference), Some(backend));
        }
        assert_eq!(reference_backend("qv1:unknown-backend:x"), None);
        assert_eq!(reference_backend("qv1:session-only:"), None);
        assert_eq!(reference_backend("qv1:session-only"), None);
        assert_eq!(reference_backend("other:session-only:x"), None);
        assert_eq!(reference_backend("qv1:session-only:has space"), None);
        assert_eq!(reference_backend("qv1:session-only:line\nbreak"), None);
        assert_eq!(validate_reference(REFERENCE), Ok(()));
    }

    #[test]
    fn secret_validation_rejects_bad_shapes_without_echoing() {
        assert_eq!(validate_secret(SECRET), Ok(()));
        for rejected in ["", "line\nbreak", "line\rbreak"] {
            assert_eq!(validate_secret(rejected), Err(SecretError::Validation));
        }
        assert_eq!(
            validate_secret(&"x".repeat(MAX_SECRET_BYTES + 1)),
            Err(SecretError::Validation)
        );
        assert_eq!(validate_secret(&"x".repeat(MAX_SECRET_BYTES)), Ok(()));
    }

    #[test]
    fn session_store_commits_only_staged_candidates() {
        let store = SessionSecrets::new();
        assert_eq!(store.backend(), SecretBackend::SessionOnly);
        assert_eq!(store.read(REFERENCE), Err(SecretError::NeedsCredentials));
        assert_eq!(
            store.write_candidate(REFERENCE),
            Err(SecretStoreError::Unavailable)
        );

        store.stage_candidate(REFERENCE, SECRET).unwrap();
        // Staging alone is not readable: only the journaled commit activates it.
        assert_eq!(store.read(REFERENCE), Err(SecretError::NeedsCredentials));
        store.write_candidate(REFERENCE).unwrap();
        assert_eq!(store.read(REFERENCE).unwrap(), SECRET);

        store.delete_reference(REFERENCE).unwrap();
        assert_eq!(store.read(REFERENCE), Err(SecretError::NeedsCredentials));
        // Deleting an absent reference succeeds so recovery stays idempotent.
        store.delete_reference(REFERENCE).unwrap();
    }

    #[test]
    fn session_store_refuses_foreign_backends_and_bad_values() {
        let store = SessionSecrets::new();
        let native = mint_reference(SecretBackend::MacosKeychain, "synthetic-entry");
        assert_eq!(
            store.stage_candidate(&native, SECRET),
            Err(SecretError::Validation)
        );
        assert_eq!(store.read(&native), Err(SecretError::Validation));
        assert_eq!(
            store.stage_candidate(REFERENCE, "line\nbreak"),
            Err(SecretError::Validation)
        );
    }

    #[test]
    fn restart_loses_every_session_value() {
        let first = SessionSecrets::new();
        first.stage_candidate(REFERENCE, SECRET).unwrap();
        first.write_candidate(REFERENCE).unwrap();
        assert_eq!(first.read(REFERENCE).unwrap(), SECRET);

        // A fresh process memory: the secret is unretrievable.
        let restarted = SessionSecrets::new();
        assert_eq!(
            restarted.read(REFERENCE),
            Err(SecretError::NeedsCredentials)
        );
    }
}
