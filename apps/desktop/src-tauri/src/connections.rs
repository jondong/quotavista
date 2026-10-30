//! Prototype connection and settings operations behind the restricted IPC
//! surface. Everything here is simulated: the synthetic provider performs no
//! real account login, no payment management, and no provider validation.
//! What is real is the safety machinery — revision compare-and-swap, lifecycle
//! fences, the deletion ledger, and the secret-store write discipline — so the
//! renderer reflects observed operation state and never optimistic success.
//!
//! Only the two secret-accepting commands ([`connect_provider`] and
//! [`replace_credentials`]) ever touch a credential value; both hand it to the
//! secret store and to nothing else. No path here writes a credential into
//! storage, logs, events, or an error.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use quotavista_contracts::{
    ErrorCode, Instant, LocalId, Locale, NextAction, OperationDto, OperationKind, OperationState,
    Polling, Provider, Revision, SafeError, SettingsDto, SettingsPatch, StorageMode,
};
use quotavista_core::calendar::parse_zone;
use quotavista_core::domain::Lifecycle;
use quotavista_core::journal::OperationKind as JournalOperationKind;
use quotavista_core::journal::SecretReferenceStore;
use quotavista_core::journal::SecretStoreError;
use quotavista_core::number::{Currency, Decimal, Ratio};
use quotavista_core::secrets::{
    mint_reference, reference_backend, SecretBackend, SecretError, SecretStore, SessionSecrets,
};
use quotavista_core::storage::{Connection, Repository, Scope, Settings, WriteContext};
use quotavista_infrastructure::secrets::reconcile_connection_credentials;

use crate::identity;
use crate::{publish_change, safe, secret_error, storage_error, AppState, LifecycleCoordinator};

/// A credential store that routes each reference to the backend its
/// `qv1:<backend>:` tag names: session-tagged references to the in-memory
/// session store, every other tag to the platform-native store. The two
/// compositions never blend — a reference minted for one backend is never
/// served by the other, and a missing native store reports
/// [`SecretError::Unavailable`] instead of substituting the session store.
pub struct RoutingSecrets {
    native: Option<Arc<dyn SecretStore>>,
    session: Arc<SessionSecrets>,
    /// Test-only injection point so the deletion-recovery flows can exercise a
    /// failing reference cleanup. Production never sets it.
    /// Test-only injection point so the deletion-recovery flows can exercise a
    /// failing reference cleanup. Production never sets it.
    fail_reference_ops: AtomicBool,
    /// Test-only injection point that fails reference *deletion* only, so a
    /// rotation can commit and then fail the superseded-reference cleanup.
    /// Production never sets it.
    fail_reference_deletes: AtomicBool,
    /// Test-only injection point that fails reference *writes* only — the
    /// candidate is staged but never committed — so a rotation can fail
    /// between the journal intent and the database commit. Production never
    /// sets it.
    fail_reference_writes: AtomicBool,
}

impl RoutingSecrets {
    /// A composition without the native store: the explicit session-only
    /// composition. References tagged for the native backend fail with
    /// [`SecretError::Unavailable`] rather than being stored in session.
    pub fn session_only() -> Self {
        Self {
            native: None,
            session: Arc::new(SessionSecrets::new()),
            fail_reference_ops: AtomicBool::new(false),
            fail_reference_deletes: AtomicBool::new(false),
            fail_reference_writes: AtomicBool::new(false),
        }
    }

    /// A composition with the native store available; session-tagged
    /// references still route to the in-memory store so the per-connection
    /// session-only mode works.
    pub fn with_native(native: Arc<dyn SecretStore>) -> Self {
        Self {
            native: Some(native),
            session: Arc::new(SessionSecrets::new()),
            fail_reference_ops: AtomicBool::new(false),
            fail_reference_deletes: AtomicBool::new(false),
            fail_reference_writes: AtomicBool::new(false),
        }
    }

    /// Backend a persistent-mode reference is minted for, or `None` in the
    /// session-only composition.
    fn native_backend(&self) -> Option<SecretBackend> {
        self.native.as_ref().map(|store| store.backend())
    }

    /// Test-only failure injection for reference cleanup.
    pub fn set_fail_reference_ops(&self, on: bool) {
        self.fail_reference_ops.store(on, Ordering::SeqCst);
    }

    /// Test-only failure injection for reference deletion only: writes still
    /// succeed, so an activation can commit before its cleanup is owed.
    pub fn set_fail_reference_deletes(&self, on: bool) {
        self.fail_reference_deletes.store(on, Ordering::SeqCst);
    }

    /// Test-only failure injection for candidate writes only: staging and
    /// deletion still work, so the failure lands between the journal intent
    /// and the store commit.
    pub fn set_fail_reference_writes(&self, on: bool) {
        self.fail_reference_writes.store(on, Ordering::SeqCst);
    }

    /// Test-only: how many staged candidates the session store still holds.
    /// Pre-journal refusals must leave no staged copy of a submitted secret
    /// behind. Native staged material is not inspectable; tests that assert
    /// this run in the session-only composition.
    #[cfg(test)]
    pub fn session_staged_count(&self) -> usize {
        self.session.staged_count()
    }

    fn check_injected_failure(&self) -> Result<(), SecretStoreError> {
        if self.fail_reference_ops.load(Ordering::SeqCst) {
            Err(SecretStoreError::Unavailable)
        } else {
            Ok(())
        }
    }

    fn route(&self, reference: &str) -> Result<Arc<dyn SecretStore>, SecretError> {
        match reference_backend(reference) {
            Some(SecretBackend::SessionOnly) => Ok(self.session.clone()),
            // Every native tag routes to the one native store this process
            // opened; references carry the tag this process minted, so a
            // mismatching tag can only come from corruption, which the native
            // store's own reference validation rejects.
            _ => self.native.clone().ok_or(SecretError::Unavailable),
        }
    }

    fn route_reference(&self, reference: &str) -> Result<Arc<dyn SecretStore>, SecretStoreError> {
        match reference_backend(reference) {
            Some(SecretBackend::SessionOnly) => Ok(self.session.clone()),
            _ => self.native.clone().ok_or(SecretStoreError::Unavailable),
        }
    }
}

impl SecretStore for RoutingSecrets {
    fn backend(&self) -> SecretBackend {
        self.native_backend().unwrap_or(SecretBackend::SessionOnly)
    }

    fn stage_candidate(&self, reference: &str, secret: &str) -> Result<(), SecretError> {
        self.check_injected_failure()
            .map_err(|_| SecretError::Unavailable)?;
        self.route(reference)?.stage_candidate(reference, secret)
    }

    fn read(&self, reference: &str) -> Result<String, SecretError> {
        self.route(reference)?.read(reference)
    }
}

impl SecretReferenceStore for RoutingSecrets {
    fn write_candidate(&self, reference: &str) -> Result<(), SecretStoreError> {
        self.check_injected_failure()?;
        if self.fail_reference_writes.load(Ordering::SeqCst) {
            return Err(SecretStoreError::Unavailable);
        }
        self.route_reference(reference)?.write_candidate(reference)
    }

    fn delete_reference(&self, reference: &str) -> Result<(), SecretStoreError> {
        self.check_injected_failure()?;
        if self.fail_reference_deletes.load(Ordering::SeqCst) {
            return Err(SecretStoreError::Unavailable);
        }
        self.route_reference(reference)?.delete_reference(reference)
    }
}

/// Map a journal failure onto its typed safe error. Journal failures keep the
/// same closed taxonomy as storage failures; nothing carries input values.
pub fn journal_error(error: &quotavista_core::journal::JournalError) -> SafeError {
    use quotavista_core::journal::JournalError;
    match error {
        JournalError::Validation => safe(ErrorCode::Validation),
        JournalError::Conflict => safe(ErrorCode::Conflict),
        JournalError::Locked => safe(ErrorCode::SecretStoreLocked),
        JournalError::Corrupt | JournalError::Durability | JournalError::RecoveryRequired => {
            safe(ErrorCode::StorageError)
        }
        JournalError::Unavailable => safe(ErrorCode::StorageError),
        JournalError::ResourceLimit => safe(ErrorCode::ResourceLimit),
    }
}

/// The degraded reason a credential command cannot run, or the typed error
/// for a composition that is degraded without a recorded reason.
fn degraded_error(state: &AppState) -> SafeError {
    state
        .degraded
        .clone()
        .unwrap_or_else(|| safe(ErrorCode::SecretStoreUnavailable))
}

/// The coordinator handle a credential command needs, or the degraded error.
fn coordinator(state: &AppState) -> Result<&tokio::sync::Mutex<LifecycleCoordinator>, SafeError> {
    state
        .coordinator
        .as_ref()
        .ok_or_else(|| degraded_error(state))
}

/// Domain provider id for a wire provider.
fn provider_id(provider: Provider) -> &'static str {
    match provider {
        Provider::Kimi => "kimi",
        Provider::Openrouter => "openrouter",
    }
}

/// Backend a new reference is minted for, honouring the connection's explicit
/// storage mode. Persistent mode in the session-only composition is a typed
/// refusal, never a silent substitution.
fn backend_for_mode(store: &RoutingSecrets, mode: StorageMode) -> Result<SecretBackend, SafeError> {
    match mode {
        StorageMode::SessionOnly => Ok(SecretBackend::SessionOnly),
        StorageMode::Persistent => store
            .native_backend()
            .ok_or_else(|| safe(ErrorCode::SecretStoreUnavailable)),
    }
}

/// Parse a wire revision into the domain generation it must match.
fn revision_i64(revision: &Revision) -> Result<i64, SafeError> {
    revision
        .as_str()
        .parse::<i64>()
        .map_err(|_| safe(ErrorCode::Validation))
}

/// Load one connection; an unknown id is a validation failure, matching the
/// refresh command's treatment of unknown ids.
async fn load_connection(
    state: &AppState,
    connection_id: &LocalId,
) -> Result<Connection, SafeError> {
    state
        .repository
        .connection(connection_id.as_str())
        .await
        .map_err(storage_error)?
        .ok_or_else(|| safe(ErrorCode::Validation))
}

/// A completed-operation descriptor the renderer can poll. The journal's own
/// projections hide completed operations, so the admission map keeps them
/// addressable.
fn completed_operation(
    operation_id: &str,
    connection_id: &str,
    kind: OperationKind,
) -> Result<OperationDto, SafeError> {
    Ok(OperationDto {
        operation_id: local_id(operation_id)?,
        connection_id: Some(local_id(connection_id)?),
        kind,
        state: OperationState::Succeeded,
        next_action: NextAction::None,
        retryable: false,
        retry_at: None,
        error: None,
    })
}

fn local_id(value: &str) -> Result<LocalId, SafeError> {
    LocalId::try_from(value.to_owned()).map_err(|_| safe(ErrorCode::InternalError))
}

/// What the repository proves about a staged candidate after an activation
/// error. The decision boundary for cleanup ownership:
///
/// - `Live`: the candidate IS the connection's credential reference. The
///   activation committed and the journal owes the superseded-reference
///   cleanup (across restarts). The caller must never touch it.
/// - `NotLive`: the repository answered and the candidate is provably not
///   the live reference. Either the coordinator's abort already deleted its
///   uncommitted candidate (journal-owned), or the activation was refused
///   before any journal intent existed — the staged copy is the caller's to
///   remove. Deleting an already-aborted candidate is idempotent.
/// - `Unknown`: the repository could not answer. Deleting could destroy a
///   committed credential, so the caller leaves everything alone; the
///   bounded lifetime of a staged copy is accepted over that risk.
enum CandidateLiveness {
    Live,
    NotLive,
    Unknown,
}

async fn candidate_liveness(
    state: &AppState,
    connection_id: &str,
    reference: &str,
) -> CandidateLiveness {
    match state.repository.credential_ref(connection_id).await {
        Ok(Some(current)) if current == reference => CandidateLiveness::Live,
        Ok(_) => CandidateLiveness::NotLive,
        Err(_) => CandidateLiveness::Unknown,
    }
}

/// Advance the store's commit counter and publish the change event for one
/// connection the mutation committed for.
async fn commit_and_publish(state: &AppState, connection_id: &str) -> Result<(), SafeError> {
    let revision_before = state
        .repository
        .data_revision()
        .await
        .map_err(storage_error)?;
    state
        .repository
        .advance_data_revision()
        .await
        .map_err(storage_error)?;
    publish_change(state, revision_before, vec![local_id(connection_id)?]).await
}

/// Add a simulated provider connection with a synthetic credential. The
/// flow is fully local: the alias and secret are validated, the credential is
/// staged under a reference minted for the chosen storage mode, the connection
/// row is created in `pending_setup`, and the coordinator's setup operation
/// activates it. No provider is contacted, so success here means the local
/// state machine completed — the UI never reads it as a validated login.
pub async fn connect_provider(
    state: &AppState,
    provider: Provider,
    alias: &quotavista_contracts::Alias,
    credential_kind: quotavista_contracts::CredentialKind,
    secret: &quotavista_contracts::SecretInput,
    storage_mode: StorageMode,
) -> Result<OperationDto, SafeError> {
    let store = state
        .secrets
        .as_ref()
        .ok_or_else(|| degraded_error(state))?;
    let coordinator = coordinator(state)?;

    // One active connection per provider: the up-front check gives a clean
    // conflict for the common case; the partial unique index covers the race.
    let provider = provider_id(provider);
    let connections = state
        .repository
        .connections()
        .await
        .map_err(storage_error)?;
    if connections
        .iter()
        .any(|c| c.provider == provider && c.lifecycle == Lifecycle::Active)
    {
        return Err(safe(ErrorCode::Conflict));
    }

    // The synthetic identity is a digest of the credential: same secret means
    // same identity, different secret means different identity. It is stored
    // as the scope key, so rotation compares identities without ever reading
    // the secret back.
    let identity = identity::synthetic_identity(secret.expose_for_provider());
    let connection_id = identity::mint_id("conn");
    let scope_id = identity::mint_id("scope");
    let operation_id = identity::mint_id("setup");
    let reference = mint_reference(
        backend_for_mode(store, storage_mode)?,
        &identity::mint_id("ref"),
    );

    // Stage the candidate first: an invalid secret (empty, oversized, or with
    // line breaks) fails here, before any row exists.
    store
        .stage_candidate(&reference, secret.expose_for_provider())
        .map_err(|error| secret_error(&error))?;
    // The credential kind is part of the wire contract but the synthetic
    // validation does not differentiate kinds; acknowledge it explicitly.
    let _ = credential_kind;

    let created = Connection {
        id: connection_id.clone(),
        provider: provider.to_owned(),
        alias: alias.as_str().to_owned(),
        generation: 0,
        lifecycle: Lifecycle::PendingSetup,
    };
    let scope = Scope {
        id: scope_id,
        connection_id: connection_id.clone(),
        key: identity,
        kind: "account".to_owned(),
    };
    if let Err(error) = state.repository.create_connection(&created, &scope).await {
        // Nothing is durable yet; drop the staged candidate so a refused
        // create (including the one-active race caught by the partial unique
        // index) leaves no orphan behind.
        let _ = store.delete_reference(&reference);
        return Err(storage_error(error));
    }

    let context = WriteContext {
        connection_id: connection_id.clone(),
        generation: 0,
    };
    let mut coordinator = coordinator.lock().await;
    if let Err(error) = coordinator
        .activate_credentials(
            &operation_id,
            &context,
            JournalOperationKind::Setup,
            &reference,
        )
        .await
    {
        // Cleanup ownership after an activation failure has three cases.
        // Committed: the candidate is the live credential reference and the
        // journal owes the superseded-reference cleanup (across restarts) —
        // never touched here. Aborted: the coordinator already deleted its
        // uncommitted candidate. Refused before any journal intent: the
        // staged copy never left this request's ownership and is removed
        // here, so a rejected setup retains no secret copy until process
        // exit. When the repository cannot prove which case applies, nothing
        // is touched — a bounded staged copy is safer than deleting a
        // credential that may have committed.
        match candidate_liveness(state, connection_id.as_str(), &reference).await {
            CandidateLiveness::Live | CandidateLiveness::Unknown => {}
            CandidateLiveness::NotLive => {
                let _ = store.delete_reference(&reference);
                let _ = state
                    .repository
                    .delete_connection(connection_id.as_str())
                    .await;
                let _ = commit_and_publish(state, &connection_id).await;
            }
        }
        return Err(journal_error(&error));
    }
    drop(coordinator);

    commit_and_publish(state, &connection_id).await?;
    let dto = completed_operation(&operation_id, &connection_id, OperationKind::Setup)?;
    Ok(state.remember_admission(dto).await)
}

/// Rotate the credential of an existing connection. A credential deriving the
/// same synthetic identity rotates in place: the coordinator's replace
/// operation stages the new reference, activates it, and deletes the
/// superseded one, while the scope — and therefore the usage history bound to
/// it — is preserved. A credential deriving a different identity never
/// silently merges into the existing scope: the rotation is refused and the
/// returned operation tells the renderer to create a replacement connection
/// instead.
pub async fn replace_credentials(
    state: &AppState,
    connection_id: &LocalId,
    expected_config_revision: &Revision,
    credential_kind: quotavista_contracts::CredentialKind,
    secret: &quotavista_contracts::SecretInput,
    storage_mode: StorageMode,
) -> Result<OperationDto, SafeError> {
    let store = state
        .secrets
        .as_ref()
        .ok_or_else(|| degraded_error(state))?;
    let coordinator = coordinator(state)?;

    let connection = load_connection(state, connection_id).await?;
    if connection.generation != revision_i64(expected_config_revision)? {
        return Err(safe(ErrorCode::Conflict));
    }
    // Rotation applies to live connections; a disabled or deleting
    // connection is a validation refusal, and a pending setup has nothing to
    // rotate.
    if !matches!(
        connection.lifecycle,
        Lifecycle::Active | Lifecycle::NeedsCredentials
    ) {
        return Err(safe(ErrorCode::Validation));
    }

    let scopes = state
        .repository
        .scopes(connection_id.as_str())
        .await
        .map_err(storage_error)?;
    let account = scopes
        .iter()
        .find(|s| s.kind == "account")
        .ok_or_else(|| safe(ErrorCode::InternalError))?;

    let new_identity = identity::synthetic_identity(secret.expose_for_provider());
    let operation_id = identity::mint_id("rotate");
    if new_identity != account.key {
        // Different (and, in this prototype, unverifiable) identity: refuse
        // the silent merge and point at the explicit replacement flow. The
        // connection is left untouched, so the working credential keeps
        // working.
        let dto = OperationDto {
            operation_id: local_id(&operation_id)?,
            connection_id: Some(connection_id.clone()),
            kind: OperationKind::ReplaceCredentials,
            state: OperationState::Failed,
            next_action: NextAction::ReplaceConnection,
            retryable: false,
            retry_at: None,
            error: Some(safe(ErrorCode::Conflict)),
        };
        return Ok(state.remember_admission(dto).await);
    }
    // Same identity: rotate in place. The credential kind participates only in
    // the wire contract, not the synthetic validation.
    let _ = credential_kind;

    let reference = mint_reference(
        backend_for_mode(store, storage_mode)?,
        &identity::mint_id("ref"),
    );
    store
        .stage_candidate(&reference, secret.expose_for_provider())
        .map_err(|error| secret_error(&error))?;

    let context = WriteContext {
        connection_id: connection_id.as_str().to_owned(),
        generation: connection.generation,
    };
    let mut coordinator = coordinator.lock().await;
    if let Err(error) = coordinator
        .activate_credentials(
            &operation_id,
            &context,
            JournalOperationKind::ReplaceCredentials,
            &reference,
        )
        .await
    {
        // Cleanup ownership after an activation failure: a committed attempt
        // keeps the candidate as the live credential (journal-owned
        // superseded cleanup, survives restarts) and an aborted attempt had
        // its uncommitted candidate deleted by the coordinator. The third
        // case is a refusal before any journal intent — for example the
        // operation fence held while an earlier cleanup is pending — where
        // the staged copy never left this request's ownership; it is
        // deleted here so a rejected rotation retains no secret copy until
        // process exit. When the repository cannot prove which case applies,
        // nothing is touched.
        if matches!(
            candidate_liveness(state, connection_id.as_str(), &reference).await,
            CandidateLiveness::NotLive
        ) {
            let _ = store.delete_reference(&reference);
        }
        return Err(journal_error(&error));
    }
    drop(coordinator);

    commit_and_publish(state, connection_id.as_str()).await?;
    let dto = completed_operation(
        &operation_id,
        connection_id.as_str(),
        OperationKind::ReplaceCredentials,
    )?;
    Ok(state.remember_admission(dto).await)
}

/// Rename a connection. Only the alias changes: the provider and identity
/// stay fixed, and the write observes the same generation, lifecycle, and
/// operation fences as every other mutation.
pub async fn update_connection(
    state: &AppState,
    connection_id: &LocalId,
    expected_config_revision: &Revision,
    alias: &quotavista_contracts::Alias,
) -> Result<quotavista_contracts::ConnectionDto, SafeError> {
    let connection = load_connection(state, connection_id).await?;
    if connection.generation != revision_i64(expected_config_revision)? {
        return Err(safe(ErrorCode::Conflict));
    }
    if connection.lifecycle == Lifecycle::DeletionPending {
        return Err(safe(ErrorCode::Conflict));
    }
    let context = WriteContext {
        connection_id: connection_id.as_str().to_owned(),
        generation: connection.generation,
    };
    // A malformed alias is a validation failure; a stale revision or an
    // operation fence is a conflict. Neither changes any state.
    state
        .repository
        .set_alias(&context, alias.as_str())
        .await
        .map_err(storage_error)?;

    commit_and_publish(state, connection_id.as_str()).await?;
    let updated = load_connection(state, connection_id).await?;
    Ok(quotavista_contracts::ConnectionDto::from_domain(
        &updated,
        Vec::new(),
        Vec::new(),
    ))
}

/// Enable or disable a connection. Disabling moves an `active` or
/// `needs_credentials` connection to `disabled`; enabling moves a `disabled`
/// connection back to `active`. Both writes bump the generation, so any late
/// refresh work against the old generation is fenced by the coordinator's
/// guards instead of publishing against stale state.
pub async fn set_connection_enabled(
    state: &AppState,
    connection_id: &LocalId,
    expected_config_revision: &Revision,
    enabled: bool,
) -> Result<quotavista_contracts::ConnectionDto, SafeError> {
    let connection = load_connection(state, connection_id).await?;
    if connection.generation != revision_i64(expected_config_revision)? {
        return Err(safe(ErrorCode::Conflict));
    }
    let target = if enabled {
        if connection.lifecycle != Lifecycle::Disabled {
            return Err(safe(ErrorCode::Validation));
        }
        Lifecycle::Active
    } else {
        if !matches!(
            connection.lifecycle,
            Lifecycle::Active | Lifecycle::NeedsCredentials
        ) {
            return Err(safe(ErrorCode::Validation));
        }
        Lifecycle::Disabled
    };

    let context = WriteContext {
        connection_id: connection_id.as_str().to_owned(),
        generation: connection.generation,
    };
    state
        .repository
        .set_lifecycle(&context, target)
        .await
        .map_err(storage_error)?;

    if enabled {
        // Enabling verifies the credential still resolves: a session-only
        // credential that did not survive a restart flips the connection to
        // `needs_credentials` instead of reporting an active connection the
        // scheduler cannot use. No credential is ever collected implicitly.
        if let Some(store) = &state.secrets {
            let routed: Arc<dyn SecretStore> = store.clone();
            let _ = reconcile_connection_credentials(
                state.repository.as_ref(),
                &routed,
                connection_id.as_str(),
            )
            .await;
        }
    }

    commit_and_publish(state, connection_id.as_str()).await?;
    let updated = load_connection(state, connection_id).await?;
    Ok(quotavista_contracts::ConnectionDto::from_domain(
        &updated,
        Vec::new(),
        Vec::new(),
    ))
}

/// Remove a connection. The coordinator fences the connection, records the
/// deletion in the ledger, and flips the lifecycle to `deletion_pending`; the
/// cleanup then deletes the owed credential reference, acknowledges the
/// ledger, and finally drops the connection row with its scopes, projections,
/// and usage history. Any cleanup failure leaves the connection in
/// `deletion_pending` with a retryable operation, and the same state is
/// reconstructed after a restart, so removal is only ever acknowledged once
/// every artifact is actually gone.
///
/// Managed-backup cleanup is explicitly not implemented in Phase 1: the
/// opaque membership rows deliberately survive this removal, and the deletion
/// guarantee they carry stays pending for the later backup phase.
pub async fn remove_connection(
    state: &AppState,
    connection_id: &LocalId,
    expected_config_revision: &Revision,
    _deletion_intent: quotavista_contracts::DeletionIntent,
) -> Result<OperationDto, SafeError> {
    let _ = state
        .secrets
        .as_ref()
        .ok_or_else(|| degraded_error(state))?;
    let coordinator = coordinator(state)?;

    let connection = load_connection(state, connection_id).await?;
    if connection.lifecycle == Lifecycle::DeletionPending {
        // A removal already in flight: the retry finishes it. The operation
        // id comes from the incomplete projection, which survives restarts.
        let pending = state
            .repository
            .operations(Some(connection_id.as_str()))
            .await
            .map_err(storage_error)?;
        match pending
            .iter()
            .find(|record| record.kind == JournalOperationKind::Delete)
        {
            Some(existing) => {
                let operation_id = existing.operation_id.clone();
                return finish_removal(state, coordinator, connection_id, &operation_id).await;
            }
            None => {
                // Recovery already ran the owed cleanup (the coordinator
                // finishes pending deletions while reopening), but the row
                // removal is this layer's job: drop it now and acknowledge.
                state
                    .repository
                    .delete_connection(connection_id.as_str())
                    .await
                    .map_err(storage_error)?;
                commit_and_publish(state, connection_id.as_str()).await?;
                let operation_id = identity::mint_id("delete");
                return completed_operation(
                    &operation_id,
                    connection_id.as_str(),
                    OperationKind::Delete,
                );
            }
        }
    }

    if connection.generation != revision_i64(expected_config_revision)? {
        return Err(safe(ErrorCode::Conflict));
    }

    let operation_id = identity::mint_id("delete");
    let context = WriteContext {
        connection_id: connection_id.as_str().to_owned(),
        generation: connection.generation,
    };
    let mut coordinator_guard = coordinator.lock().await;
    coordinator_guard
        .begin_deletion(&operation_id, &context)
        .await
        .map_err(|error| journal_error(&error))?;
    drop(coordinator_guard);

    finish_removal(state, coordinator, connection_id, &operation_id).await
}

/// Run the cleanup half of a removal: delete the owed reference, acknowledge
/// the ledger, then drop the connection row. A cleanup failure is reported as
/// a failed, retryable operation with the connection still `deletion_pending`.
async fn finish_removal(
    state: &AppState,
    coordinator: &tokio::sync::Mutex<LifecycleCoordinator>,
    connection_id: &LocalId,
    operation_id: &str,
) -> Result<OperationDto, SafeError> {
    let mut coordinator_guard = coordinator.lock().await;
    if let Err(error) = coordinator_guard.finish_deletion(operation_id).await {
        let error = journal_error(&error);
        let dto = OperationDto {
            operation_id: local_id(operation_id)?,
            connection_id: Some(connection_id.clone()),
            kind: OperationKind::Delete,
            state: OperationState::Failed,
            next_action: NextAction::Retry,
            retryable: true,
            retry_at: retry_at_hint(),
            error: Some(error),
        };
        return Ok(state.remember_admission(dto).await);
    }
    drop(coordinator_guard);

    state
        .repository
        .delete_connection(connection_id.as_str())
        .await
        .map_err(storage_error)?;
    commit_and_publish(state, connection_id.as_str()).await?;
    let dto = completed_operation(operation_id, connection_id.as_str(), OperationKind::Delete)?;
    Ok(state.remember_admission(dto).await)
}

/// A short retry hint for a failed cleanup: the value only suggests when the
/// renderer may poll again, never what happened.
fn retry_at_hint() -> Option<Instant> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() + 5)
        .unwrap_or(0);
    let formatted =
        chrono::DateTime::<chrono::Utc>::from_timestamp(i64::try_from(now).unwrap_or(0), 0)
            .map(|instant| instant.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    formatted.and_then(|text| Instant::try_from(text).ok())
}

/// Update the versioned settings. Every field is optional: a field left out
/// or sent as `null` keeps its current value, so the canonical patch always
/// carries all fields. A patch with no changes is a read that returns the
/// current settings without touching the revision, because an empty change
/// set cannot overwrite a newer choice. A non-empty patch requires the
/// caller's expected revision to equal the current one — a stale revision is
/// a conflict, never a silent overwrite.
pub async fn update_settings(
    state: &AppState,
    expected_settings_revision: &Revision,
    changes: &SettingsPatch,
) -> Result<SettingsDto, SafeError> {
    let current = state.repository.settings().await.map_err(storage_error)?;

    let locale = match changes.ui_locale {
        Some(Locale::En) => Some("en".to_owned()),
        Some(Locale::ZhCn) => Some("zh-CN".to_owned()),
        None => None,
    };
    let reporting_zone = match &changes.reporting_zone {
        Some(zone) => Some(parse_zone(zone.as_str()).map_err(|_| safe(ErrorCode::Validation))?),
        None => None,
    };
    let currency = match &changes.display_currency {
        Some(dto) => Some(
            dto.as_str()
                .parse::<Currency>()
                .map_err(|_| safe(ErrorCode::Validation))?,
        ),
        None => None,
    };
    let refresh_minutes = changes.polling.map(|polling| match polling {
        Polling::Disabled => None,
        Polling::OneMinute => Some(1),
        Polling::FiveMinutes => Some(5),
        Polling::FifteenMinutes => Some(15),
    });
    let quota_threshold = match &changes.quota_threshold {
        Some(dto) => Some(
            dto.as_str()
                .parse::<Ratio>()
                .map_err(|_| safe(ErrorCode::Validation))?,
        ),
        None => None,
    };
    let balance_threshold = match &changes.balance_threshold {
        Some(dto) => Some(
            dto.as_str()
                .parse::<Decimal>()
                .map_err(|_| safe(ErrorCode::Validation))?,
        ),
        None => None,
    };

    let touched = locale.is_some()
        || reporting_zone.is_some()
        || currency.is_some()
        || refresh_minutes.is_some()
        || quota_threshold.is_some()
        || balance_threshold.is_some();
    if !touched {
        return SettingsDto::from_domain(&current);
    }

    // Optimistic revision: the caller must have seen the revision it is
    // overwriting. The database's compare-and-swap below covers the window
    // between this read and the write.
    if revision_i64(expected_settings_revision)? != current.revision {
        return Err(safe(ErrorCode::Conflict));
    }

    let next = Settings {
        revision: current.revision,
        locale: locale.unwrap_or_else(|| current.locale.clone()),
        reporting_zone: reporting_zone.unwrap_or(current.reporting_zone),
        currency: currency.unwrap_or(current.currency),
        refresh_minutes: refresh_minutes.unwrap_or(current.refresh_minutes),
        quota_threshold: quota_threshold.unwrap_or(current.quota_threshold),
        balance_threshold: balance_threshold.unwrap_or(current.balance_threshold),
    };
    let saved = state
        .repository
        .save_settings(&next)
        .await
        .map_err(storage_error)?;
    SettingsDto::from_domain(&saved)
}

/// Reconcile every active connection against its credential store at
/// composition start. A session-only credential never survives a restart, so
/// its connection reports `needs_credentials` from the first list the
/// renderer sees, without any implicit credential collection. Locked or
/// unavailable stores leave connections untouched: those are store states,
/// not missing credentials.
pub async fn reconcile_startup_credentials(
    repository: &quotavista_infrastructure::sqlite::SqliteRepository,
    store: &Arc<RoutingSecrets>,
) {
    let routed: Arc<dyn SecretStore> = store.clone();
    let connections = match repository.connections().await {
        Ok(connections) => connections,
        Err(_) => return,
    };
    for connection in connections {
        if connection.lifecycle == Lifecycle::Active {
            let _ = reconcile_connection_credentials(repository, &routed, &connection.id).await;
        }
    }
}
