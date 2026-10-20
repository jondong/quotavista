//! Native secret store adapters behind pinned keyring-core, plus the startup
//! needs-credentials reconciliation.
//!
//! [`NativeSecrets`] talks to the macOS Keychain or the Linux Secret Service
//! through the pinned `keyring-core` API. Every call runs on a dedicated
//! worker thread, so a blocking keychain prompt or a slow D-Bus round-trip can
//! never stall an async runtime or a UI thread. Only opaque references cross
//! the port boundary; secret values exist in this module and the backend only.
//!
//! Locked and unavailable are distinct end to end: a locked store leaves work
//! visibly pending with no downgrade to session-only or plaintext storage, and
//! a missing entry reads as [`SecretError::NeedsCredentials`]. Errors carry no
//! platform detail and no part of a secret value.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use quotavista_core::journal::SecretStoreError;
use quotavista_core::{
    domain::Lifecycle,
    secrets::{SecretError, SecretStore},
    storage::{Repository, StorageError, WriteContext},
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use quotavista_core::{
    journal::SecretReferenceStore,
    secrets::{reference_backend, validate_reference, validate_secret, SecretBackend},
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::collections::BTreeMap;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::thread::JoinHandle;

/// Service name the production native stores write under.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const SERVICE: &str = "quotavista";

/// Reconcile one connection's retrievability at startup: read its reference
/// and transition it to needs-credentials when no secret can be retrieved. A
/// session-only connection always lands here after a restart, because its
/// value never reached disk. Returns whether a secret is retrievable.
///
/// A locked or unavailable store reports its error and leaves the connection
/// untouched: the state stays explicit and nothing downgrades to session-only
/// or plaintext storage.
pub async fn reconcile_connection_credentials<R: Repository>(
    repository: &R,
    store: &Arc<dyn SecretStore>,
    connection_id: &str,
) -> Result<bool, SecretError> {
    let connection = repository
        .connection(connection_id)
        .await
        .map_err(storage_error)?
        .ok_or(SecretError::Validation)?;
    if connection.lifecycle != Lifecycle::Active {
        return Ok(false);
    }
    let reference = repository
        .credential_ref(connection_id)
        .await
        .map_err(storage_error)?;
    let retrievable = match reference.as_deref() {
        // An active connection with no reference at all has no credential
        // material either way.
        None => false,
        Some(reference) => {
            // The read can stall on a keychain prompt or a D-Bus round-trip:
            // keep it off the async worker. The caller runs on a tokio runtime.
            let store = store.clone();
            let reference = reference.to_owned();
            match tokio::task::spawn_blocking(move || store.read(&reference)).await {
                Ok(Ok(_)) => true,
                Ok(Err(SecretError::NeedsCredentials)) => false,
                Ok(Err(error)) => return Err(error),
                Err(_) => return Err(SecretError::Unavailable),
            }
        }
    };
    if retrievable {
        return Ok(true);
    }
    let context = WriteContext {
        connection_id: connection_id.to_owned(),
        generation: connection.generation,
    };
    match repository
        .set_lifecycle(&context, Lifecycle::NeedsCredentials)
        .await
    {
        Ok(_) => Ok(false),
        // A concurrent mutation already moved the generation; the caller's
        // retry sees the new state.
        Err(StorageError::Conflict) => Ok(false),
        Err(error) => Err(storage_error(error)),
    }
}

fn storage_error(error: StorageError) -> SecretError {
    match error {
        StorageError::Unavailable => SecretError::Unavailable,
        _ => SecretError::Validation,
    }
}

/// Native secret store: macOS Keychain or Linux Secret Service behind pinned
/// keyring-core, with every blocking call isolated on a dedicated worker.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub struct NativeSecrets {
    backend: SecretBackend,
    sender: Option<Sender<Command>>,
    worker: Option<JoinHandle<()>>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
enum Command {
    Stage {
        reference: String,
        secret: String,
        reply: Sender<Result<(), SecretError>>,
    },
    Write {
        reference: String,
        reply: Sender<Result<(), SecretError>>,
    },
    Read {
        reference: String,
        reply: Sender<Result<String, SecretError>>,
    },
    Delete {
        reference: String,
        reply: Sender<Result<(), SecretError>>,
    },
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl NativeSecrets {
    /// Open the platform's default native store under the production service
    /// name. The constructor probes the backend, so a missing keychain daemon
    /// or session bus fails here as [`SecretError::Unavailable`].
    pub fn open() -> Result<Self, SecretError> {
        Self::open_with_service(SERVICE)
    }

    /// Open the native store under an explicit service name. Exists so smoke
    /// checks can run under a disposable `quotavista-test-` service without
    /// touching production entries.
    pub fn open_with_service(service: &str) -> Result<Self, SecretError> {
        let backend = default_backend();
        let store: Box<dyn Backend> = Box::new(KeyringBackend {
            store: open_keyring_store()?,
        });
        let (sender, receiver) = channel();
        let worker_service = service.to_owned();
        let worker = std::thread::spawn(move || run(store, backend, worker_service, receiver));
        Ok(Self {
            backend,
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    /// Send one command to the worker and wait for its reply. A dead worker or
    /// a dropped reply channel means the backend is unreachable.
    fn exchange<T>(
        &self,
        command: impl FnOnce(Sender<Result<T, SecretError>>) -> Command,
    ) -> Result<T, SecretError> {
        let (reply, receiver) = channel();
        let sender = self.sender.as_ref().ok_or(SecretError::Unavailable)?;
        sender
            .send(command(reply))
            .map_err(|_| SecretError::Unavailable)?;
        receiver.recv().map_err(|_| SecretError::Unavailable)?
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for NativeSecrets {
    fn drop(&mut self) {
        // Closing the channel ends the worker's receive loop.
        drop(self.sender.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl SecretStore for NativeSecrets {
    fn backend(&self) -> SecretBackend {
        self.backend
    }

    /// Staging holds the validated value in worker memory only; the journaled
    /// [`SecretReferenceStore::write_candidate`] commits it to the backend.
    fn stage_candidate(&self, reference: &str, secret: &str) -> Result<(), SecretError> {
        self.check_reference(reference)?;
        validate_secret(secret)?;
        self.exchange(|reply| Command::Stage {
            reference: reference.to_owned(),
            secret: secret.to_owned(),
            reply,
        })
    }

    fn read(&self, reference: &str) -> Result<String, SecretError> {
        self.check_reference(reference)?;
        self.exchange(|reply| Command::Read {
            reference: reference.to_owned(),
            reply,
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl NativeSecrets {
    fn check_reference(&self, reference: &str) -> Result<(), SecretError> {
        validate_reference(reference)?;
        if reference_backend(reference) != Some(self.backend) {
            return Err(SecretError::Validation);
        }
        Ok(())
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl SecretReferenceStore for NativeSecrets {
    fn write_candidate(&self, reference: &str) -> Result<(), SecretStoreError> {
        let result = self.exchange(|reply| Command::Write {
            reference: reference.to_owned(),
            reply,
        });
        result.map_err(reference_error)
    }

    fn delete_reference(&self, reference: &str) -> Result<(), SecretStoreError> {
        let result = self.exchange(|reply| Command::Delete {
            reference: reference.to_owned(),
            reply,
        });
        result.map_err(reference_error)
    }
}

/// The reference-only port cannot express needs-credentials or validation, so
/// anything but a locked store collapses to unavailable.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn reference_error(error: SecretError) -> SecretStoreError {
    match error {
        SecretError::Locked => SecretStoreError::Locked,
        SecretError::Unavailable | SecretError::NeedsCredentials | SecretError::Validation => {
            SecretStoreError::Unavailable
        }
    }
}

#[cfg(target_os = "macos")]
fn default_backend() -> SecretBackend {
    SecretBackend::MacosKeychain
}

#[cfg(target_os = "linux")]
fn default_backend() -> SecretBackend {
    SecretBackend::LinuxSecretService
}

#[cfg(target_os = "macos")]
fn open_keyring_store() -> Result<Arc<keyring_core::CredentialStore>, SecretError> {
    apple_native_keyring_store::keychain::Store::new()
        .map(|store| store as Arc<keyring_core::CredentialStore>)
        .map_err(map_keyring_error)
}

#[cfg(target_os = "linux")]
fn open_keyring_store() -> Result<Arc<keyring_core::CredentialStore>, SecretError> {
    dbus_secret_service_keyring_store::Store::new()
        .map(|store| store as Arc<keyring_core::CredentialStore>)
        .map_err(map_keyring_error)
}

/// Map pinned keyring-core 1.0.1 errors onto the port taxonomy. Denied or
/// dismissed access is locked; a missing entry reads as needs-credentials;
/// everything else, including platform failures and absent storage, is
/// unavailable. Nothing platform-specific or value-bearing crosses.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn map_keyring_error(error: keyring_core::Error) -> SecretError {
    match error {
        keyring_core::Error::NoStorageAccess(_) => SecretError::Locked,
        keyring_core::Error::NoEntry => SecretError::NeedsCredentials,
        keyring_core::Error::TooLong(_, _)
        | keyring_core::Error::Invalid(_, _)
        | keyring_core::Error::BadEncoding(_) => SecretError::Validation,
        keyring_core::Error::PlatformFailure(_)
        | keyring_core::Error::BadDataFormat(..)
        | keyring_core::Error::BadStoreFormat(_)
        | keyring_core::Error::Ambiguous(_)
        | keyring_core::Error::NoDefaultStore
        | keyring_core::Error::NotSupportedByStore(_) => SecretError::Unavailable,
        // keyring-core is non-exhaustive; unknown future variants are
        // unavailable, never locked.
        _ => SecretError::Unavailable,
    }
}

/// The platform backend behind the worker. The keyring adapter implements it;
/// tests substitute a fake, so the staging and cleanup rules are exercised
/// deterministically.
#[cfg(any(target_os = "macos", target_os = "linux"))]
trait Backend: Send {
    fn write(&self, service: &str, reference: &str, value: &str) -> Result<(), SecretError>;
    fn read(&self, service: &str, reference: &str) -> Result<String, SecretError>;
    fn delete(&self, service: &str, reference: &str) -> Result<(), SecretError>;
}

/// keyring-core backend: the pinned macOS Keychain or Linux Secret Service
/// store behind [`Arc<keyring_core::CredentialStore>`].
#[cfg(any(target_os = "macos", target_os = "linux"))]
struct KeyringBackend {
    store: Arc<keyring_core::CredentialStore>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl KeyringBackend {
    /// Build the keyring entry for a reference: the reference itself is the
    /// account name, so entries are namespaced by the `qv1` scheme and the
    /// backend tag.
    fn entry(&self, service: &str, reference: &str) -> Result<keyring_core::Entry, SecretError> {
        self.store
            .build(service, reference, None)
            .map_err(map_keyring_error)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Backend for KeyringBackend {
    fn write(&self, service: &str, reference: &str, value: &str) -> Result<(), SecretError> {
        self.entry(service, reference).and_then(|entry| {
            entry
                .set_secret(value.as_bytes())
                .map_err(map_keyring_error)
        })
    }

    fn read(&self, service: &str, reference: &str) -> Result<String, SecretError> {
        self.entry(service, reference)
            .and_then(|entry| entry.get_secret().map_err(map_keyring_error))
            .and_then(|bytes| String::from_utf8(bytes).map_err(|_| SecretError::Unavailable))
    }

    fn delete(&self, service: &str, reference: &str) -> Result<(), SecretError> {
        match self.entry(service, reference) {
            Ok(entry) => match entry.delete_credential() {
                // Deleting an absent reference succeeds, so recovery resumes
                // cleanup idempotently.
                Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
                Err(error) => Err(map_keyring_error(error)),
            },
            Err(error) => Err(error),
        }
    }
}

/// Worker loop: owns the backend handle and the staged values, so the calling
/// thread never blocks on the platform backend.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run(
    backend: Box<dyn Backend>,
    kind: SecretBackend,
    service: String,
    receiver: Receiver<Command>,
) {
    let mut staged: BTreeMap<String, String> = BTreeMap::new();
    while let Ok(command) = receiver.recv() {
        handle(backend.as_ref(), kind, &service, &mut staged, command);
    }
}

/// Handle one command against the staged values and the backend.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn handle(
    backend: &dyn Backend,
    kind: SecretBackend,
    service: &str,
    staged: &mut BTreeMap<String, String>,
    command: Command,
) {
    match command {
        Command::Stage {
            reference,
            secret,
            reply,
        } => {
            let result = if reference_backend(&reference) == Some(kind) {
                staged.insert(reference, secret);
                Ok(())
            } else {
                Err(SecretError::Validation)
            };
            let _ = reply.send(result);
        }
        Command::Write { reference, reply } => {
            // The staging slot is always consumed, so a failed commit cannot
            // leak a value into a later write.
            let result = match staged.remove(&reference) {
                Some(value) => backend.write(service, &reference, &value),
                None => Err(SecretError::Unavailable),
            };
            let _ = reply.send(result);
        }
        Command::Read { reference, reply } => {
            let _ = reply.send(backend.read(service, &reference));
        }
        Command::Delete { reference, reply } => {
            // The cleanup postcondition: the staged bytes are dropped with the
            // entry, so a later write of the same reference cannot resurrect
            // them. A backend error is still reported, so durable cleanup
            // stays retryable.
            staged.remove(&reference);
            let _ = reply.send(backend.delete(service, &reference));
        }
    }
}

#[cfg(test)]
mod tests;
