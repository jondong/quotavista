//! Desktop composition root: assembles core, providers, and infrastructure
//! behind restricted Tauri IPC. The renderer never touches storage, secrets,
//! providers, or the coordinator directly; it sends a single validated
//! [`quotavista_contracts::Request`] and receives an [`Envelope`].
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use quotavista_contracts::{
    ChangeEvent, Command, CommandResult, ConnectionDto, ConnectionList, Envelope, ErrorCode,
    Instant, LocalId, NextAction, OperationDto, OperationKind, OperationList, OperationState,
    RefreshTarget, Request, Revision, SafeError, CONTRACT_VERSION,
};
use quotavista_core::journal::SecretReferenceStore;
use quotavista_core::provider::{ProviderError, RefreshClock};
use quotavista_core::secrets::{SecretError, SecretStore};
use quotavista_core::storage::{Repository, StorageError, WriteContext};
use quotavista_infrastructure::durable::{FileStore, SharedFileStore};
use quotavista_infrastructure::refresh::{Admission, HistoryOutcome, Outcome, RefreshCoordinator};
use quotavista_infrastructure::sqlite::SqliteRepository;
use quotavista_providers::simulated::SimulatedAdapter;
use quotavista_providers::synthetic_providers;
use tauri::{Emitter, Manager};

/// UTC clock used by the coordinator and by [`Request::validate`].
#[derive(Clone, Copy, Debug)]
pub struct UtcClock;

impl quotavista_core::calendar::Clock for UtcClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl RefreshClock for UtcClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Sink for revision-only change events. Production wires this to the Tauri
/// `qv://change` channel; tests record events into a vector. The renderer
/// never sees values: every event carries opaque identifiers and revisions
/// only, so a leak through the event channel cannot surface credentials or
/// prose.
pub trait ChangeSink: Send + Sync {
    fn emit(&self, event: ChangeEvent);
}

struct TauriChangeSink {
    handle: tauri::AppHandle,
}

impl ChangeSink for TauriChangeSink {
    fn emit(&self, event: ChangeEvent) {
        // Failure to emit must not derail the caller: the channel is an
        // invalidation hint, so a dropped event is a missed reload, not a
        // data loss.
        let _ = self.handle.emit("qv://change", &event);
    }
}

/// Records the emitted change events so tests can assert that an event is
/// published only after a commit.
#[cfg(test)]
pub struct RecordingChangeSink {
    events: std::sync::Mutex<Vec<ChangeEvent>>,
}

#[cfg(test)]
impl RecordingChangeSink {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<ChangeEvent> {
        self.events.lock().expect("change sink lock").clone()
    }
}

#[cfg(test)]
impl Default for RecordingChangeSink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl ChangeSink for RecordingChangeSink {
    fn emit(&self, event: ChangeEvent) {
        self.events.lock().expect("change sink lock").push(event);
    }
}

/// The credential store the desktop uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretChoice {
    /// The platform-native store (macOS keychain, Linux Secret Service). This
    /// is the default; a native store that cannot be opened leaves the
    /// composition degraded instead of being replaced.
    Native,
    /// The in-memory session store. Selecting it is a deliberate opt-in and
    /// never a fallback for a locked or missing native store.
    SessionOnly,
}

/// Environment variable that records the explicit session-only opt-in.
pub const SESSION_ONLY_ENV: &str = "QUOTAVISTA_SECRET_STORE";

/// Resolve the credential store choice. The native store is the default;
/// `QUOTAVISTA_SECRET_STORE=session-only` is the only way to select the
/// in-memory store, and it is a visible, deliberate opt-in.
pub fn secret_choice_from_env() -> SecretChoice {
    match std::env::var(SESSION_ONLY_ENV).as_deref() {
        Ok("session-only") => SecretChoice::SessionOnly,
        _ => SecretChoice::Native,
    }
}

/// Open the platform-native credential store.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn open_native_secrets() -> Result<Arc<dyn SecretStore>, SecretError> {
    Ok(Arc::new(
        quotavista_infrastructure::secrets::NativeSecrets::open()?,
    ))
}

/// No native backend exists on this platform, so the composition runs
/// degraded until the user selects a store explicitly.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn open_native_secrets() -> Result<Arc<dyn SecretStore>, SecretError> {
    Err(SecretError::Unavailable)
}

/// Everything the renderer can reach through Tauri commands. Held in Tauri's
/// managed state so command handlers can borrow it without re-opening stores
/// (which would defeat the lifecycle fence and the cooldown journal).
pub struct AppState {
    pub repository: Arc<SqliteRepository>,
    /// The selected credential store. `None` while the composition runs
    /// degraded: the native store could not be opened, and the session store
    /// is never substituted for it.
    pub secrets: Option<Arc<dyn SecretStore>>,
    /// Typed reason the composition runs degraded, if it does.
    pub degraded: Option<SafeError>,
    /// Lifecycle coordinator. It needs a credential reference store, so it is
    /// absent while the composition runs degraded.
    pub coordinator: Option<tokio::sync::Mutex<LifecycleCoordinator>>,
    pub refresh: tokio::sync::Mutex<RefreshState>,
    /// Refresh job id to connection id, so a polled operation can name the
    /// connection it belongs to.
    pub jobs: tokio::sync::Mutex<BTreeMap<String, String>>,
    /// Operations that were reported without a coordinator job (a refused or
    /// cooldown-blocked admission). Keeping them makes every operation id the
    /// refresh command returns pollable.
    pub admissions: tokio::sync::Mutex<BTreeMap<String, OperationDto>>,
    /// Counter behind the synthetic operation ids of admissions without a job.
    pub admission_counter: std::sync::atomic::AtomicU64,
    pub changes: Arc<dyn ChangeSink>,
}
impl AppState {
    /// Synthetic operation id for an admission that has no coordinator job.
    fn next_admission_id(&self) -> LocalId {
        let sequence = self.admission_counter.fetch_add(1, Ordering::SeqCst) + 1;
        LocalId::try_from(format!("admission-{sequence}")).expect("synthetic LocalId shape")
    }

    /// Keep an admission without a job so its operation id stays pollable.
    async fn remember_admission(&self, dto: OperationDto) -> OperationDto {
        self.admissions
            .lock()
            .await
            .insert(dto.operation_id.as_str().to_owned(), dto.clone());
        dto
    }
}

/// The lifecycle coordinator wired to the SQLite store, the file journal, and
/// the credential reference store.
type LifecycleCoordinator = quotavista_infrastructure::lifecycle::Coordinator<
    SqliteRepository,
    SharedFileStore,
    Arc<dyn SecretReferenceStore>,
>;

/// The refresh coordinator wired to the synthetic provider adapter.
type RefreshState =
    RefreshCoordinator<SqliteRepository, SimulatedAdapter, SharedFileStore, UtcClock>;

/// Initialise the desktop composition in a caller-supplied root directory.
/// The desktop binary uses `std::env::temp_dir()`; tests pass a
/// `tempfile::TempDir` so no real application directory is ever touched.
pub async fn build(
    root: PathBuf,
    changes: Arc<dyn ChangeSink>,
    secrets: SecretChoice,
) -> Result<AppState, SafeError> {
    build_with_provider(
        root,
        changes,
        secrets,
        SimulatedAdapter::new(synthetic_providers()[0]),
    )
    .await
}

/// Initialise the desktop composition with an explicit provider adapter.
/// Tests use this to inject a failing synthetic scenario.
pub async fn build_with_provider(
    root: PathBuf,
    changes: Arc<dyn ChangeSink>,
    secrets: SecretChoice,
    provider: SimulatedAdapter,
) -> Result<AppState, SafeError> {
    build_inner(root, changes, open_choice(secrets), provider).await
}

/// Open the chosen credential store. The in-memory session store is only
/// reachable through an explicit choice; it is never a fallback.
fn open_choice(choice: SecretChoice) -> Result<Arc<dyn SecretStore>, SecretError> {
    match choice {
        SecretChoice::SessionOnly => Ok(Arc::new(quotavista_core::secrets::SessionSecrets::new())),
        SecretChoice::Native => open_native_secrets(),
    }
}

async fn build_inner(
    root: PathBuf,
    changes: Arc<dyn ChangeSink>,
    opened: Result<Arc<dyn SecretStore>, SecretError>,
    provider: SimulatedAdapter,
) -> Result<AppState, SafeError> {
    std::fs::create_dir_all(&root).map_err(|_| safe(ErrorCode::InternalError))?;
    let database = SqliteRepository::open(&root.join("quotavista.sqlite"))
        .await
        .map_err(|_| safe(ErrorCode::StorageError))?;
    let durable = SharedFileStore(Arc::new(
        FileStore::new(root.join("journal")).map_err(|_| safe(ErrorCode::InternalError))?,
    ));

    // A credential store that could not be opened leaves the composition
    // degraded: the typed error is kept and reported, and the session store
    // is never substituted for it.
    let (store, degraded): (Option<Arc<dyn SecretStore>>, Option<SafeError>) = match opened {
        Ok(store) => (Some(store), None),
        Err(error) => (None, Some(secret_error(&error))),
    };

    let coordinator = match &store {
        Some(store) => {
            let references: Arc<dyn SecretReferenceStore> = store.clone();
            let mut coordinator = quotavista_infrastructure::lifecycle::Coordinator::open(
                database.clone(),
                durable.clone(),
                references,
            )
            .map_err(|_| safe(ErrorCode::InternalError))?;
            coordinator
                .recover()
                .await
                .map_err(|_| safe(ErrorCode::InternalError))?;
            Some(tokio::sync::Mutex::new(coordinator))
        }
        None => None,
    };

    let refresh = RefreshCoordinator::open(database.clone(), provider, durable.clone(), UtcClock)
        .map_err(|_| safe(ErrorCode::InternalError))?;

    Ok(AppState {
        repository: Arc::new(database),
        secrets: store,
        degraded,
        coordinator,
        refresh: tokio::sync::Mutex::new(refresh),
        jobs: tokio::sync::Mutex::new(BTreeMap::new()),
        admissions: tokio::sync::Mutex::new(BTreeMap::new()),
        admission_counter: std::sync::atomic::AtomicU64::new(0),
        changes,
    })
}

/// Initialise the desktop composition with a Tauri AppHandle. The AppHandle
/// is captured into a [`TauriChangeSink`] so committed changes broadcast a
/// revision-only `qv://change` event to every subscribed window.
pub async fn build_for_tauri(
    root: PathBuf,
    handle: tauri::AppHandle,
) -> Result<AppState, SafeError> {
    let sink: Arc<dyn ChangeSink> = Arc::new(TauriChangeSink { handle });
    build(root, sink, secret_choice_from_env()).await
}

fn safe(code: ErrorCode) -> SafeError {
    SafeError::new(code, None)
}
/// Map a credential-store failure onto its typed safe error. A locked store and
/// an unavailable store stay distinct: a locked store waits for the user, an
/// unavailable one means the backend is missing.
fn secret_error(error: &SecretError) -> SafeError {
    match error {
        SecretError::Locked => safe(ErrorCode::SecretStoreLocked),
        SecretError::Unavailable => safe(ErrorCode::SecretStoreUnavailable),
        SecretError::NeedsCredentials => safe(ErrorCode::NeedsCredentials),
        SecretError::Validation => safe(ErrorCode::Validation),
    }
}

/// The single dispatch surface the renderer may call. Every other entrypoint
/// in this module is private; the secret-input contract is uniform because the
/// renderer can never reach a handler that accepts credentials outside this
/// dispatcher.
///
/// The request is accepted as an optional raw JSON value so the application
/// owns the whole decode boundary. A missing, malformed, or oversized payload
/// never reaches Tauri's `CommandArg` machinery; the application returns a
/// sealed [`Envelope::Error`] with a fixed message instead of echoing the
/// input.
#[tauri::command]
async fn dispatch_command(
    state: tauri::State<'_, AppState>,
    request: Option<serde_json::Value>,
) -> Result<Envelope, Envelope> {
    Ok(run_request(state.inner(), request).await)
}

/// Decode, validate, and run one request. The Tauri command and the tests
/// share this path, so a test exercises the production behaviour and not a
/// parallel implementation.
async fn run_request(state: &AppState, request: Option<serde_json::Value>) -> Envelope {
    let request = match request {
        Some(value) => match serde_json::from_value::<Request>(value) {
            Ok(request) => request,
            // Decode failures stay inside the application envelope: the
            // renderer never sees the framework's source-traced error path,
            // and the safe message never echoes the offending value, field
            // name, or provider.
            Err(_) => return envelope_error(synthetic_request_id(), safe(ErrorCode::Validation)),
        },
        // A missing argument is answered with the same sealed error as a
        // malformed one.
        None => return envelope_error(synthetic_request_id(), safe(ErrorCode::Validation)),
    };

    if let Err(error) = request.validate(&UtcClock) {
        return envelope_error(request.request_id.clone(), error);
    }
    let request_id = request.request_id.clone();
    match dispatch(state, request).await {
        Ok(data) => Envelope::Ok {
            contract_version: CONTRACT_VERSION,
            request_id,
            data: Box::new(data),
        },
        Err(error) => envelope_error(request_id, error),
    }
}

/// Run a validated request against the composition.
pub async fn dispatch(state: &AppState, request: Request) -> Result<CommandResult, SafeError> {
    match request.command {
        Command::ListConnections => list_connections(state)
            .await
            .map(CommandResult::ListConnections),
        Command::ConnectProvider { .. } | Command::ReplaceCredentials { .. } => {
            // The credential store is the only reason these two commands
            // cannot be served yet, so a degraded composition reports that
            // first instead of a generic "not implemented yet".
            match &state.degraded {
                Some(error) => Err(error.clone()),
                None => Err(safe(ErrorCode::UnsupportedInPhaseOne)),
            }
        }
        Command::UpdateConnection { .. } => Err(safe(ErrorCode::UnsupportedInPhaseOne)),
        Command::SetConnectionEnabled { .. } => Err(safe(ErrorCode::UnsupportedInPhaseOne)),
        Command::RemoveConnection { .. } => Err(safe(ErrorCode::UnsupportedInPhaseOne)),
        Command::RefreshConnections { target } => refresh_connections(state, target)
            .await
            .map(CommandResult::RefreshConnections),
        // The overview and statistics queries belong to the later analytics
        // phase. They report that state instead of an invented success.
        Command::GetOverview { .. } => Err(safe(ErrorCode::UnsupportedInPhaseOne)),
        Command::QueryStatistics { .. } => Err(safe(ErrorCode::UnsupportedInPhaseOne)),
        Command::UpdateSettings { .. } => Err(safe(ErrorCode::UnsupportedInPhaseOne)),
        Command::GetOperation { operation_id } => get_operation(state, operation_id)
            .await
            .map(CommandResult::GetOperation),
    }
}

fn envelope_error(request_id: LocalId, error: SafeError) -> Envelope {
    Envelope::Error {
        contract_version: CONTRACT_VERSION,
        request_id,
        error,
    }
}

/// Stable id returned with decode-failure envelopes. The renderer keys its
/// own diagnostics on this value, not on any user-provided field, so a
/// malformed envelope can never leak the offending input.
fn synthetic_request_id() -> LocalId {
    LocalId::try_from("qv-malformed".to_owned()).expect("static LocalId literal")
}

async fn list_connections(state: &AppState) -> Result<ConnectionList, SafeError> {
    let connections = state
        .repository
        .connections()
        .await
        .map_err(storage_error)?;
    let dtos = connections
        .iter()
        .map(|c| ConnectionDto::from_domain(c, Vec::new(), Vec::new()))
        .collect();
    Ok(ConnectionList {
        connections: dtos,
        data_revision: data_revision(state).await?,
    })
}

/// The store-wide data revision. Every committed snapshot or published
/// partition advances it, so it is the monotonic revision the change events
/// carry.
async fn data_revision(state: &AppState) -> Result<Revision, SafeError> {
    let revision = state
        .repository
        .data_revision()
        .await
        .map_err(storage_error)?;
    Revision::try_from(revision.max(0).to_string()).map_err(|_| safe(ErrorCode::InternalError))
}

/// Publish a change event when a mutating step advanced the store's commit
/// counter. Every path that can commit goes through here - the refresh request
/// and the poll that advances an unfinished job - so a view that settles after
/// an event still learns about the commits that follow it.
///
/// `affected` names the connections the step committed for: the refresh request
/// names the connections whose job completed in its batch, and the poll names
/// the connections whose job completed during that poll.
async fn publish_change(
    state: &AppState,
    revision_before: i64,
    affected: Vec<LocalId>,
) -> Result<(), SafeError> {
    let revision_after = state
        .repository
        .data_revision()
        .await
        .map_err(storage_error)?;
    if revision_after <= revision_before {
        return Ok(());
    }
    let mut affected_scopes = Vec::new();
    for connection in &affected {
        for scope in state
            .repository
            .scope_ids(connection.as_str())
            .await
            .map_err(storage_error)?
        {
            if let Ok(scope) = LocalId::try_from(scope) {
                affected_scopes.push(scope);
            }
        }
    }
    state.changes.emit(ChangeEvent {
        data_revision: Revision::try_from(revision_after.max(0).to_string())
            .map_err(|_| safe(ErrorCode::InternalError))?,
        affected_connection_ids: affected,
        affected_scope_ids: affected_scopes,
    });
    Ok(())
}

/// The connections of the given coordinator jobs, taken from the mapping the
/// refresh request recorded when it admitted them.
async fn job_connections(state: &AppState, jobs: &[String]) -> Vec<LocalId> {
    let mapping = state.jobs.lock().await;
    let mut connections: Vec<LocalId> = Vec::new();
    for job in jobs {
        if let Some(connection) = mapping
            .get(job)
            .and_then(|id| LocalId::try_from(id.clone()).ok())
        {
            if !connections.contains(&connection) {
                connections.push(connection);
            }
        }
    }
    connections
}

/// Upper bound on the coordinator steps one refresh request drives. The
/// synthetic provider needs eight; the bound only keeps a provider that never
/// terminates from stalling the request.
const MAX_REFRESH_STEPS: usize = 64;

async fn refresh_connections(
    state: &AppState,
    target: RefreshTarget,
) -> Result<OperationList, SafeError> {
    let connections = state
        .repository
        .connections()
        .await
        .map_err(storage_error)?;

    // An unknown selected id is rejected before any job is admitted: dropping
    // it silently would refresh a different set than the caller asked for.
    let selected: Vec<&quotavista_core::storage::Connection> = match &target {
        RefreshTarget::All => connections.iter().collect(),
        RefreshTarget::Selected(ids) => {
            let known: BTreeSet<&str> = connections.iter().map(|c| c.id.as_str()).collect();
            if ids.iter().any(|id| !known.contains(id.as_str())) {
                return Err(safe(ErrorCode::Validation));
            }
            let wanted: BTreeSet<&str> = ids.iter().map(|id| id.as_str()).collect();
            connections
                .iter()
                .filter(|c| wanted.contains(c.id.as_str()))
                .collect()
        }
    };

    let requests: Vec<(String, WriteContext)> = selected
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                WriteContext {
                    connection_id: c.id.clone(),
                    generation: c.generation,
                },
            )
        })
        .collect();
    let request_refs: Vec<(&str, WriteContext)> = requests
        .iter()
        .map(|(id, ctx)| (id.as_str(), ctx.clone()))
        .collect();

    let revision_before = state
        .repository
        .data_revision()
        .await
        .map_err(storage_error)?;

    let mut refresh = state.refresh.lock().await;
    let admissions = refresh.request_manual(&request_refs).await;

    // Drive the admitted jobs far enough that the returned operation ids are
    // pollable and a committed revision can be reported.
    for _ in 0..MAX_REFRESH_STEPS {
        let running = admissions.iter().any(|admission| match admission {
            Admission::Admitted { job } | Admission::Coalesced { job } => {
                refresh.outcome(job).is_none()
            }
            _ => false,
        });
        if !running {
            break;
        }
        refresh.pump().await;
    }

    let mut operations = Vec::with_capacity(admissions.len());
    let mut affected_connections: Vec<LocalId> = Vec::new();

    for (admission, (connection_id, _)) in admissions.iter().zip(requests.iter()) {
        let local_id =
            LocalId::try_from(connection_id.clone()).map_err(|_| safe(ErrorCode::Validation))?;
        match admission {
            Admission::Admitted { job } | Admission::Coalesced { job } => {
                let job_id =
                    LocalId::try_from(job.clone()).map_err(|_| safe(ErrorCode::InternalError))?;
                state
                    .jobs
                    .lock()
                    .await
                    .insert(job.clone(), connection_id.clone());
                let outcome = refresh.outcome(job);
                let committed = matches!(outcome, Some(Outcome::Completed { .. }));
                operations.push(operation_dto(
                    job_id,
                    Some(local_id.clone()),
                    outcome.as_ref(),
                )?);
                if committed {
                    affected_connections.push(local_id);
                }
            }
            Admission::CooldownBlocked { until } => {
                let retry_at = Instant::try_from(until.to_rfc3339_opts(SecondsFormat::Secs, true))
                    .map_err(|_| safe(ErrorCode::InternalError))?;
                let dto = OperationDto {
                    operation_id: state.next_admission_id(),
                    connection_id: Some(local_id),
                    kind: OperationKind::Refresh,
                    state: OperationState::Pending,
                    next_action: NextAction::Retry,
                    retryable: true,
                    retry_at: Some(retry_at.clone()),
                    error: Some(SafeError::new(ErrorCode::RateLimited, Some(retry_at))),
                };
                operations.push(state.remember_admission(dto).await);
            }
            Admission::Refused => {
                let dto = OperationDto {
                    operation_id: state.next_admission_id(),
                    connection_id: Some(local_id),
                    kind: OperationKind::Refresh,
                    state: OperationState::Failed,
                    next_action: NextAction::None,
                    retryable: false,
                    retry_at: None,
                    error: Some(SafeError::new(ErrorCode::Conflict, None)),
                };
                operations.push(state.remember_admission(dto).await);
            }
        }
    }
    drop(refresh);

    // The event is published only when a commit advanced the store revision,
    // so a refresh that failed or wrote nothing announces no change.
    publish_change(state, revision_before, affected_connections).await?;

    Ok(OperationList { operations })
}

async fn get_operation(state: &AppState, operation_id: LocalId) -> Result<OperationDto, SafeError> {
    let target = operation_id.as_str().to_owned();
    let revision_before = state
        .repository
        .data_revision()
        .await
        .map_err(storage_error)?;
    let mut polled: Option<(Option<LocalId>, Option<Outcome>)> = None;
    let mut committed_jobs: Vec<String> = Vec::new();
    {
        let mut refresh = state.refresh.lock().await;
        let job_ids = refresh.job_ids();
        if job_ids.iter().any(|job| job == &target) {
            // The jobs that are still running are the ones that can commit
            // during this poll.
            let running: Vec<String> = job_ids
                .iter()
                .filter(|job| refresh.outcome(job).is_none())
                .cloned()
                .collect();
            if refresh.outcome(&target).is_none() {
                // A job that is still running is advanced by the poll itself, so
                // polling is the continuation path for a job that outlived the
                // refresh command's own step budget.
                refresh.pump().await;
            }
            for job in running {
                if matches!(refresh.outcome(&job), Some(Outcome::Completed { .. })) {
                    committed_jobs.push(job);
                }
            }
            let outcome = refresh.outcome(&target);
            let connection = state
                .jobs
                .lock()
                .await
                .get(&target)
                .cloned()
                .and_then(|id| LocalId::try_from(id).ok());
            polled = Some((connection, outcome));
        }
    }
    if let Some((connection, outcome)) = polled {
        // The poll advanced the coordinator and may have committed, so it
        // announces its change like every other commit path. Without it a view
        // that settles after the event would keep a stale revision.
        let affected = job_connections(state, &committed_jobs).await;
        publish_change(state, revision_before, affected).await?;
        return operation_dto(operation_id, connection, outcome.as_ref());
    }
    // An admission without a coordinator job keeps its operation id pollable.
    if let Some(dto) = state.admissions.lock().await.get(&target).cloned() {
        return Ok(dto);
    }
    // Not a refresh job: fall back to the durable operation ledger.
    let operations = state
        .repository
        .operations(None)
        .await
        .map_err(storage_error)?;
    let record = operations
        .into_iter()
        .find(|record| record.operation_id == target)
        .ok_or_else(|| safe(ErrorCode::Validation))?;
    Ok(OperationDto::from_record(&record))
}

/// Map a refresh outcome onto the operation the renderer polls. A job that is
/// still running stays `Running` so the caller can poll it again.
fn operation_dto(
    operation_id: LocalId,
    connection_id: Option<LocalId>,
    outcome: Option<&Outcome>,
) -> Result<OperationDto, SafeError> {
    let mut dto = OperationDto {
        operation_id,
        connection_id,
        kind: OperationKind::Refresh,
        state: OperationState::Running,
        next_action: NextAction::None,
        retryable: false,
        retry_at: None,
        error: None,
    };
    let Some(outcome) = outcome else {
        return Ok(dto);
    };
    match outcome {
        Outcome::Completed { history } => match history {
            HistoryOutcome::Published { .. } | HistoryOutcome::Skipped => {
                dto.state = OperationState::Succeeded;
            }
            // The history stream never attested completeness, so nothing was
            // published. The refresh must not report synchronization success.
            HistoryOutcome::Incomplete => {
                dto.state = OperationState::Failed;
                dto.next_action = NextAction::Retry;
                dto.retryable = true;
                dto.error = Some(SafeError::new(ErrorCode::InvalidResponse, None));
            }
        },
        Outcome::Cancelled => {
            dto.state = OperationState::Cancelled;
            dto.retryable = true;
            dto.error = Some(SafeError::new(ErrorCode::Cancelled, None));
        }
        Outcome::Fenced => {
            dto.state = OperationState::Failed;
            dto.next_action = NextAction::Retry;
            dto.retryable = true;
            dto.error = Some(SafeError::new(ErrorCode::Conflict, None));
        }
        Outcome::Cooldown { until } => {
            let retry_at = Instant::try_from(until.to_rfc3339_opts(SecondsFormat::Secs, true))
                .map_err(|_| safe(ErrorCode::InternalError))?;
            dto.state = OperationState::Pending;
            dto.next_action = NextAction::Retry;
            dto.retryable = true;
            dto.retry_at = Some(retry_at.clone());
            dto.error = Some(SafeError::new(ErrorCode::RateLimited, Some(retry_at)));
        }
        Outcome::Failed { error } => {
            dto.state = OperationState::Failed;
            dto.retryable = true;
            dto.error = Some(provider_error(error));
            dto.next_action = match error {
                ProviderError::PermissionRequired => NextAction::ProvideCredentials,
                _ => NextAction::Retry,
            };
        }
        Outcome::StorageFailed => {
            dto.state = OperationState::Failed;
            dto.next_action = NextAction::Retry;
            dto.retryable = true;
            dto.error = Some(SafeError::new(ErrorCode::StorageError, None));
        }
    }
    Ok(dto)
}

fn provider_error(error: &ProviderError) -> SafeError {
    match error {
        ProviderError::RateLimited { .. } => SafeError::new(ErrorCode::RateLimited, None),
        ProviderError::PermissionRequired => SafeError::new(ErrorCode::PermissionRequired, None),
        ProviderError::Offline => SafeError::new(ErrorCode::Offline, None),
        ProviderError::Timeout => SafeError::new(ErrorCode::Timeout, None),
        ProviderError::InvalidResponse => SafeError::new(ErrorCode::InvalidResponse, None),
    }
}

fn storage_error(error: StorageError) -> SafeError {
    match error {
        StorageError::Conflict => SafeError::new(ErrorCode::Conflict, None),
        StorageError::Validation => SafeError::new(ErrorCode::Validation, None),
        StorageError::Migration | StorageError::IncompatibleVersion => {
            SafeError::new(ErrorCode::InternalError, None)
        }
        StorageError::Unavailable => SafeError::new(ErrorCode::StorageError, None),
    }
}

/// Build, persist, and run the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("desktop runtime must start");
    let root = std::env::temp_dir().join("quotavista-prototype");
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![dispatch_command])
        .setup(move |app| {
            let handle = app.handle().clone();
            let state = runtime
                .block_on(build_for_tauri(root.clone(), handle))
                .expect("desktop composition must initialise");
            if let Some(error) = &state.degraded {
                // The credential store is unavailable: report the typed error
                // instead of quietly using a weaker store.
                eprintln!(
                    "quotavista: credential store unavailable: {}",
                    error.message()
                );
            }
            app.manage(state);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building QuotaVista prototype");
    app.run(|_handle, _event| {});
}

#[cfg(test)]
mod tests {
    use super::*;
    use quotavista_contracts::{Alias, Provider, SecretInput};
    use quotavista_core::domain::Lifecycle;
    use quotavista_core::storage::{Connection, Scope};
    use quotavista_providers::simulated::Scenario;
    use tempfile::TempDir;

    const CONNECTION_ID: &str = "synthetic-connection-0";

    /// Hermetic fixture: a throwaway directory, the in-memory session store
    /// chosen explicitly, and the recording change sink. No keychain, Secret
    /// Service, or application directory is touched.
    async fn fixture() -> (TempDir, AppState, Arc<RecordingChangeSink>) {
        fixture_with_provider(SimulatedAdapter::new(synthetic_providers()[0])).await
    }

    async fn fixture_with_provider(
        provider: SimulatedAdapter,
    ) -> (TempDir, AppState, Arc<RecordingChangeSink>) {
        let dir = tempfile::tempdir().expect("temp dir");
        let sink = Arc::new(RecordingChangeSink::new());
        let changes: Arc<dyn ChangeSink> = sink.clone();
        let state = build_with_provider(
            dir.path().to_path_buf(),
            changes,
            SecretChoice::SessionOnly,
            provider,
        )
        .await
        .expect("composition must initialise");
        (dir, state, sink)
    }

    /// Add a connection with one scope so the coordinator can admit a job for
    /// it.
    async fn seed_connection(state: &AppState, lifecycle: Lifecycle) {
        seed_connection_with_id(state, CONNECTION_ID, lifecycle).await;
    }

    async fn seed_connection_with_id(state: &AppState, id: &str, lifecycle: Lifecycle) {
        state
            .repository
            .create_connection(
                &Connection {
                    id: id.to_owned(),
                    provider: format!("synthetic-provider-{id}"),
                    alias: format!("Synthetic {id}"),
                    generation: 0,
                    lifecycle,
                },
                &Scope {
                    id: format!("synthetic-scope-{id}"),
                    connection_id: id.to_owned(),
                    key: format!("synthetic-account-{id}"),
                    kind: "account".to_owned(),
                },
            )
            .await
            .expect("connection must be stored");
    }

    fn local_id(value: &str) -> LocalId {
        LocalId::try_from(value.to_owned()).expect("valid LocalId literal")
    }

    fn revision(value: u64) -> Revision {
        Revision::try_from(value.to_string()).expect("valid Revision literal")
    }

    /// The canonical wire shape of a request, exactly as the renderer sends
    /// it: the command stays a nested tagged object.
    fn canonical(command: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "contract_version": CONTRACT_VERSION,
            "request_id": "fixture-request",
            "command": command,
        })
    }

    fn list_connections_request() -> serde_json::Value {
        canonical(serde_json::json!({ "command": "list_connections" }))
    }

    fn refresh_all_request() -> serde_json::Value {
        canonical(serde_json::json!({
            "command": "refresh_connections",
            "args": { "target": { "kind": "all" } },
        }))
    }

    fn refresh_selected_request(ids: &[&str]) -> serde_json::Value {
        canonical(serde_json::json!({
            "command": "refresh_connections",
            "args": { "target": { "kind": "selected", "connection_ids": ids } },
        }))
    }

    fn get_operation_request(operation_id: &str) -> serde_json::Value {
        canonical(serde_json::json!({
            "command": "get_operation",
            "args": { "operation_id": operation_id },
        }))
    }

    /// The single operation of a refresh envelope.
    fn only_operation(envelope: &Envelope) -> OperationDto {
        match envelope {
            Envelope::Ok { data, .. } => match data.as_ref() {
                CommandResult::RefreshConnections(list) => {
                    assert_eq!(list.operations.len(), 1);
                    list.operations[0].clone()
                }
                other => panic!("expected RefreshConnections, got {other:?}"),
            },
            other => panic!("expected Envelope::Ok, got {other:?}"),
        }
    }

    fn expect_error(envelope: &Envelope, code: ErrorCode) {
        match envelope {
            Envelope::Error { error, .. } => assert_eq!(error.code(), code),
            other => panic!("expected Envelope::Error with {code:?}, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_rejects_unknown_contract_version() {
        let request = Request {
            contract_version: 999,
            request_id: local_id("fixture"),
            command: Command::ListConnections,
        };
        let error = request
            .validate(&UtcClock)
            .expect_err("rejects unknown version");
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[tokio::test]
    async fn oversized_secret_is_rejected_while_a_valid_credential_decodes() {
        // The canonical nested request shape accepts a one-byte credential, so
        // the rejection below is about the size and not about the shape.
        let valid = canonical(serde_json::json!({
            "command": "connect_provider",
            "args": {
                "provider": "kimi",
                "alias": "Demo",
                "credential_kind": "api_key",
                "secret": "x",
                "storage_mode": "session_only",
            },
        }));
        assert!(
            serde_json::from_value::<Request>(valid).is_ok(),
            "a valid one-byte credential must decode"
        );

        let oversized = canonical(serde_json::json!({
            "command": "connect_provider",
            "args": {
                "provider": "kimi",
                "alias": "Demo",
                "credential_kind": "api_key",
                "secret": "x".repeat(64 * 1024 + 1),
                "storage_mode": "session_only",
            },
        }));
        assert!(
            serde_json::from_value::<Request>(oversized).is_err(),
            "an oversized credential must fail before validation"
        );
    }

    #[tokio::test]
    async fn dispatch_rejects_oversized_scope_set() {
        let mut ids = Vec::new();
        for i in 0..101 {
            ids.push(local_id(&format!("scope-{i}")));
        }
        let request = Request {
            contract_version: CONTRACT_VERSION,
            request_id: local_id("fixture-request"),
            command: Command::RefreshConnections {
                target: RefreshTarget::Selected(ids),
            },
        };
        let error = request.validate(&UtcClock).expect_err("101 ids must fail");
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[tokio::test]
    async fn dispatch_rejects_empty_selected_target() {
        let request = Request {
            contract_version: CONTRACT_VERSION,
            request_id: local_id("fixture-request"),
            command: Command::RefreshConnections {
                target: RefreshTarget::Selected(Vec::new()),
            },
        };
        let error = request
            .validate(&UtcClock)
            .expect_err("empty selection must fail");
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[tokio::test]
    async fn only_connect_and_replace_accept_secret_input() {
        fn decode(json: serde_json::Value) -> Command {
            serde_json::from_value::<Command>(json).expect("command must decode")
        }

        let raw_connect = serde_json::json!({
            "command": "connect_provider",
            "args": {
                "provider": "kimi",
                "alias": "Demo",
                "credential_kind": "api_key",
                "secret": "synthetic-fixture-only",
                "storage_mode": "session_only",
            }
        });
        let raw_replace = serde_json::json!({
            "command": "replace_credentials",
            "args": {
                "connection_id": "c",
                "expected_config_revision": "1",
                "credential_kind": "api_key",
                "secret": "synthetic-fixture-only",
                "storage_mode": "persistent",
            }
        });
        let raw_update = serde_json::json!({
            "command": "update_connection",
            "args": {
                "connection_id": "c",
                "expected_config_revision": "1",
                "alias": "Demo",
            }
        });
        let raw_enable = serde_json::json!({
            "command": "set_connection_enabled",
            "args": {
                "connection_id": "c",
                "expected_config_revision": "1",
                "enabled": true,
            }
        });
        let raw_remove = serde_json::json!({
            "command": "remove_connection",
            "args": {
                "connection_id": "c",
                "expected_config_revision": "1",
                "deletion_intent": "credentials_history_and_managed_backups",
            }
        });
        let raw_refresh = serde_json::json!({
            "command": "refresh_connections",
            "args": { "target": { "kind": "all" } }
        });
        let raw_get_operation =
            serde_json::json!({ "command": "get_operation", "args": { "operation_id": "o" } });

        for raw in [
            &raw_update,
            &raw_enable,
            &raw_remove,
            &raw_refresh,
            &raw_get_operation,
        ] {
            let mut with_secret = raw.clone();
            with_secret["args"]["secret"] = serde_json::Value::String("smuggled".into());
            assert!(
                serde_json::from_value::<Command>(with_secret).is_err(),
                "raw={raw} must reject a smuggled secret field"
            );
        }

        assert!(matches!(
            decode(raw_connect),
            Command::ConnectProvider { .. }
        ));
        assert!(matches!(
            decode(raw_replace),
            Command::ReplaceCredentials { .. }
        ));
    }

    #[tokio::test]
    async fn malformed_request_answers_with_a_sealed_error() {
        let (_dir, state, _sink) = fixture().await;
        // A numeric secret must not reach the error message.
        let raw = canonical(serde_json::json!({
            "command": "connect_provider",
            "args": {
                "provider": "kimi",
                "alias": "Demo",
                "credential_kind": "api_key",
                "secret": 123456789,
                "storage_mode": "session_only",
            },
        }));
        let envelope = run_request(&state, Some(raw)).await;
        match envelope {
            Envelope::Error {
                error, request_id, ..
            } => {
                assert_eq!(error.code(), ErrorCode::Validation);
                assert_eq!(request_id, local_id("qv-malformed"));
                assert!(!error.message().contains("123456789"));
            }
            other => panic!("malformed input must produce Envelope::Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_request_answers_with_a_sealed_error() {
        let (_dir, state, _sink) = fixture().await;
        let envelope = run_request(&state, None).await;
        match envelope {
            Envelope::Error {
                error, request_id, ..
            } => {
                assert_eq!(error.code(), ErrorCode::Validation);
                assert_eq!(request_id, local_id("qv-malformed"));
            }
            other => panic!("missing input must produce Envelope::Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_connections_returns_persisted_state() {
        let (_dir, state, _sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        let envelope = run_request(&state, Some(list_connections_request())).await;
        match envelope {
            Envelope::Ok { data, .. } => match *data {
                CommandResult::ListConnections(list) => {
                    assert_eq!(list.connections.len(), 1);
                    assert_eq!(list.connections[0].connection_id, local_id(CONNECTION_ID));
                    // The revision is the store revision, not the settings
                    // revision.
                    assert_eq!(list.data_revision, revision(0));
                }
                other => panic!("expected ListConnections, got {other:?}"),
            },
            other => panic!("expected Envelope::Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn refresh_rejects_an_unknown_selected_connection() {
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        let envelope = run_request(
            &state,
            Some(refresh_selected_request(&["synthetic-unknown"])),
        )
        .await;
        expect_error(&envelope, ErrorCode::Validation);
        // Nothing was admitted, so no change event was published.
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn refresh_drives_the_coordinator_and_returns_pollable_job_ids() {
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;

        let envelope = run_request(&state, Some(refresh_all_request())).await;
        let operation = only_operation(&envelope);

        // The operation id is the coordinator job id, not the connection id.
        assert_ne!(operation.operation_id, local_id(CONNECTION_ID));
        assert_eq!(operation.connection_id, Some(local_id(CONNECTION_ID)));
        assert_eq!(operation.state, OperationState::Succeeded);

        // The same id polls the same operation.
        let polled = run_request(
            &state,
            Some(get_operation_request(operation.operation_id.as_str())),
        )
        .await;
        match polled {
            Envelope::Ok { data, .. } => match *data {
                CommandResult::GetOperation(polled) => {
                    assert_eq!(polled.operation_id, operation.operation_id);
                    assert_eq!(polled.state, OperationState::Succeeded);
                }
                other => panic!("expected GetOperation, got {other:?}"),
            },
            other => panic!("expected Envelope::Ok, got {other:?}"),
        }

        // The commit advanced the store revision, so exactly one change event
        // was published and it carries that revision.
        let events = sink.events();
        assert_eq!(events.len(), 1);
        let committed = state
            .repository
            .data_revision()
            .await
            .expect("store revision");
        assert_eq!(
            events[0].data_revision,
            Revision::try_from(committed.max(0).to_string()).expect("revision")
        );
        assert_eq!(
            events[0].affected_connection_ids,
            vec![local_id(CONNECTION_ID)]
        );
        assert!(!events[0].affected_scope_ids.is_empty());
    }

    #[tokio::test]
    async fn a_failed_refresh_publishes_no_change_event() {
        let adapter = SimulatedAdapter::new(synthetic_providers()[0]);
        adapter.set_scenario(Scenario::RateLimited);
        let (_dir, state, sink) = fixture_with_provider(adapter).await;
        seed_connection(&state, Lifecycle::Active).await;

        let envelope = run_request(&state, Some(refresh_all_request())).await;
        let operation = only_operation(&envelope);
        // A rate-limited provider never reports synchronization success.
        assert_eq!(operation.state, OperationState::Pending);
        assert!(operation.retryable);
        assert_eq!(
            operation.error.as_ref().map(|error| error.code()),
            Some(ErrorCode::RateLimited)
        );
        // Nothing was committed, so no revision event was published.
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn a_refused_connection_reports_a_failure_without_an_event() {
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::PendingSetup).await;

        let envelope = run_request(&state, Some(refresh_all_request())).await;
        let operation = only_operation(&envelope);
        assert_eq!(operation.state, OperationState::Failed);
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn every_returned_operation_id_can_be_polled() {
        let (_dir, state, _sink) = fixture().await;
        seed_connection(&state, Lifecycle::PendingSetup).await;

        let envelope = run_request(&state, Some(refresh_all_request())).await;
        let operation = only_operation(&envelope);
        assert_eq!(operation.state, OperationState::Failed);

        // A refused admission has no coordinator job, but the operation id it
        // hands out stays pollable.
        let polled = run_request(
            &state,
            Some(get_operation_request(operation.operation_id.as_str())),
        )
        .await;
        match polled {
            Envelope::Ok { data, .. } => match *data {
                CommandResult::GetOperation(polled) => {
                    assert_eq!(polled.operation_id, operation.operation_id);
                    assert_eq!(polled.state, OperationState::Failed);
                }
                other => panic!("expected GetOperation, got {other:?}"),
            },
            other => panic!("expected Envelope::Ok, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn change_event_revisions_only_move_forward() {
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;

        run_request(&state, Some(refresh_all_request())).await;
        run_request(&state, Some(refresh_all_request())).await;

        let events = sink.events();
        assert_eq!(events.len(), 2);
        let first: u64 = events[0].data_revision.as_str().parse().expect("revision");
        let second: u64 = events[1].data_revision.as_str().parse().expect("revision");
        assert!(
            second > first,
            "the store revision must move forward: {first} then {second}"
        );
    }

    #[tokio::test]
    async fn get_operation_rejects_missing_id() {
        let (_dir, state, _sink) = fixture().await;
        let envelope = run_request(&state, Some(get_operation_request("missing"))).await;
        expect_error(&envelope, ErrorCode::Validation);
    }

    #[tokio::test]
    async fn later_phase_commands_return_typed_unsupported_state() {
        let (_dir, state, _sink) = fixture().await;
        // Every command that a later phase owns, sent through the same
        // validated request path the renderer uses.
        let commands = [
            serde_json::json!({
                "command": "connect_provider",
                "args": {
                    "provider": "kimi",
                    "alias": "Demo",
                    "credential_kind": "api_key",
                    "secret": "synthetic-fixture-only",
                    "storage_mode": "session_only",
                },
            }),
            serde_json::json!({
                "command": "update_connection",
                "args": {
                    "connection_id": "synthetic-connection-0",
                    "expected_config_revision": "1",
                    "alias": "Demo",
                },
            }),
            serde_json::json!({
                "command": "replace_credentials",
                "args": {
                    "connection_id": "synthetic-connection-0",
                    "expected_config_revision": "1",
                    "credential_kind": "api_key",
                    "secret": "synthetic-fixture-only",
                    "storage_mode": "persistent",
                },
            }),
            serde_json::json!({
                "command": "set_connection_enabled",
                "args": {
                    "connection_id": "synthetic-connection-0",
                    "expected_config_revision": "1",
                    "enabled": true,
                },
            }),
            serde_json::json!({
                "command": "remove_connection",
                "args": {
                    "connection_id": "synthetic-connection-0",
                    "expected_config_revision": "1",
                    "deletion_intent": "credentials_history_and_managed_backups",
                },
            }),
            serde_json::json!({
                "command": "update_settings",
                "args": {
                    "expected_settings_revision": "1",
                    "changes": {},
                },
            }),
            serde_json::json!({
                "command": "get_overview",
                "args": {
                    "range": {
                        "start_date": "2024-01-01",
                        "end_date": "2024-01-07",
                        "reporting_zone": "UTC",
                        "scope_ids": null,
                    },
                },
            }),
            serde_json::json!({
                "command": "query_statistics",
                "args": {
                    "range": {
                        "start_date": "2024-01-01",
                        "end_date": "2024-01-07",
                        "reporting_zone": "UTC",
                        "grain": "day",
                        "scope_ids": null,
                    },
                },
            }),
        ];
        assert_eq!(commands.len(), 8, "every later-phase command is covered");
        for command in commands {
            let envelope = run_request(&state, Some(canonical(command))).await;
            expect_error(&envelope, ErrorCode::UnsupportedInPhaseOne);
        }
    }

    #[tokio::test]
    async fn a_degraded_composition_reports_the_credential_store_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let sink = Arc::new(RecordingChangeSink::new());
        let changes: Arc<dyn ChangeSink> = sink.clone();
        // A credential store that cannot be opened leaves the composition
        // degraded instead of falling back to the session store.
        let state = build_inner(
            dir.path().to_path_buf(),
            changes,
            Err(SecretError::Unavailable),
            SimulatedAdapter::new(synthetic_providers()[0]),
        )
        .await
        .expect("composition must initialise");
        assert!(state.secrets.is_none());
        assert_eq!(
            state.degraded.as_ref().map(|error| error.code()),
            Some(ErrorCode::SecretStoreUnavailable)
        );

        // The commands that need the credential store report the typed error.
        let request = Request {
            contract_version: CONTRACT_VERSION,
            request_id: local_id("fixture-request"),
            command: Command::ConnectProvider {
                provider: Provider::Kimi,
                alias: Alias::try_from("Demo".to_owned()).unwrap(),
                credential_kind: quotavista_contracts::CredentialKind::ApiKey,
                secret: SecretInput::try_from("synthetic-only".to_owned()).unwrap(),
                storage_mode: quotavista_contracts::StorageMode::SessionOnly,
            },
        };
        let error = dispatch(&state, request)
            .await
            .expect_err("a degraded composition must refuse");
        assert_eq!(error.code(), ErrorCode::SecretStoreUnavailable);

        // Cached local data still reads back.
        seed_connection(&state, Lifecycle::Active).await;
        let envelope = run_request(&state, Some(list_connections_request())).await;
        match envelope {
            Envelope::Ok { data, .. } => match *data {
                CommandResult::ListConnections(list) => assert_eq!(list.connections.len(), 1),
                other => panic!("expected ListConnections, got {other:?}"),
            },
            other => panic!("expected Envelope::Ok, got {other:?}"),
        }
    }

    /// The generated handler is the transport the renderer uses. This test
    /// sends the exact argument shape the renderer sends and reads the
    /// envelope the renderer reads.
    #[test]
    fn the_generated_handler_accepts_the_renderer_argument_shape() {
        let dir = tempfile::tempdir().expect("temp dir");
        let sink = Arc::new(RecordingChangeSink::new());
        let changes: Arc<dyn ChangeSink> = sink.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let state = runtime
            .block_on(build_with_provider(
                dir.path().to_path_buf(),
                changes,
                SecretChoice::SessionOnly,
                SimulatedAdapter::new(synthetic_providers()[0]),
            ))
            .expect("composition must initialise");

        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![dispatch_command])
            .manage(state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock window");

        // The renderer sends `invoke("dispatch_command", { request })`.
        let body = serde_json::json!({
            "request": {
                "contract_version": CONTRACT_VERSION,
                "request_id": "renderer-request",
                "command": { "command": "list_connections" },
            }
        });
        let response = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "dispatch_command".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().expect("url"),
                body: tauri::ipc::InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .expect("invoke response");
        match response {
            tauri::ipc::InvokeResponseBody::Json(body) => {
                let value: serde_json::Value =
                    serde_json::from_str(&body).expect("json response body");
                assert_eq!(value["status"], "ok");
                assert_eq!(value["request_id"], "renderer-request");
            }
            other => panic!("expected a JSON body, got {other:?}"),
        }
    }

    /// A missing argument is answered inside the application envelope instead
    /// of failing the framework's argument binding.
    #[test]
    fn the_generated_handler_answers_a_missing_argument_with_an_error_envelope() {
        let dir = tempfile::tempdir().expect("temp dir");
        let sink = Arc::new(RecordingChangeSink::new());
        let changes: Arc<dyn ChangeSink> = sink.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let state = runtime
            .block_on(build_with_provider(
                dir.path().to_path_buf(),
                changes,
                SecretChoice::SessionOnly,
                SimulatedAdapter::new(synthetic_providers()[0]),
            ))
            .expect("composition must initialise");

        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![dispatch_command])
            .manage(state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock window");

        let response = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "dispatch_command".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().expect("url"),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({})),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .expect("invoke response");
        match response {
            tauri::ipc::InvokeResponseBody::Json(body) => {
                let value: serde_json::Value =
                    serde_json::from_str(&body).expect("json response body");
                assert_eq!(value["status"], "error");
                assert_eq!(value["error"]["code"], "validation");
            }
            other => panic!("expected a JSON body, got {other:?}"),
        }
    }

    /// A credential-store failure keeps its own code: a locked store and an
    /// unavailable store are different states for the user.
    #[test]
    fn secret_errors_keep_locked_and_unavailable_distinct() {
        assert_eq!(
            secret_error(&SecretError::Locked).code(),
            ErrorCode::SecretStoreLocked
        );
        assert_eq!(
            secret_error(&SecretError::Unavailable).code(),
            ErrorCode::SecretStoreUnavailable
        );
        assert_eq!(
            secret_error(&SecretError::NeedsCredentials).code(),
            ErrorCode::NeedsCredentials
        );
        assert_eq!(
            secret_error(&SecretError::Validation).code(),
            ErrorCode::Validation
        );
    }

    /// A job that outlives the refresh command's step budget is finished by
    /// polling: the poll advances the coordinator, so a caller is never left
    /// with an operation that can no longer progress.
    #[tokio::test]
    async fn polling_finishes_a_job_that_outlived_the_refresh_budget() {
        let (_dir, state, sink) = fixture().await;
        // The coordinator runs at most two jobs at a time, so more connections
        // than the budget can carry leave jobs behind.
        for index in 0..24 {
            seed_connection_with_id(
                &state,
                &format!("synthetic-connection-{index}"),
                Lifecycle::Active,
            )
            .await;
        }

        let envelope = run_request(&state, Some(refresh_all_request())).await;
        let operations = match envelope {
            Envelope::Ok { data, .. } => match *data {
                CommandResult::RefreshConnections(list) => list.operations,
                other => panic!("expected RefreshConnections, got {other:?}"),
            },
            other => panic!("expected Envelope::Ok, got {other:?}"),
        };
        let mut pending: Vec<OperationDto> = operations
            .into_iter()
            .filter(|operation| operation.state == OperationState::Running)
            .collect();
        assert!(
            !pending.is_empty(),
            "the refresh budget must leave a job running"
        );

        // Polling is the continuation path: every poll advances the
        // coordinator, so the jobs the refresh request could not carry finish
        // here.
        for _ in 0..200 {
            if pending.is_empty() {
                break;
            }
            let mut still_running = Vec::new();
            for operation in pending {
                let envelope = run_request(
                    &state,
                    Some(get_operation_request(operation.operation_id.as_str())),
                )
                .await;
                let polled = match envelope {
                    Envelope::Ok { data, .. } => match *data {
                        CommandResult::GetOperation(operation) => operation,
                        other => panic!("expected GetOperation, got {other:?}"),
                    },
                    other => panic!("expected Envelope::Ok, got {other:?}"),
                };
                if polled.state == OperationState::Running {
                    still_running.push(polled);
                }
            }
            pending = still_running;
        }
        assert!(
            pending.is_empty(),
            "every job must finish when it is polled"
        );

        // The polls made the commits the refresh request could not, and they
        // announce them: the last announced revision is the committed one.
        let events = sink.events();
        assert!(
            events.len() > 1,
            "the polls must announce the commits they made"
        );
        let announced: Vec<u64> = events
            .iter()
            .map(|event| {
                event
                    .data_revision
                    .as_str()
                    .parse()
                    .expect("a decimal revision")
            })
            .collect();
        assert!(
            announced.windows(2).all(|pair| pair[0] < pair[1]),
            "announced revisions must move forward"
        );
        let committed = state
            .repository
            .data_revision()
            .await
            .expect("store revision");
        let last = *announced.last().expect("at least one event");
        assert_eq!(
            last, committed as u64,
            "the last announced revision must be the committed revision"
        );
    }

    /// A commit the store refuses must not announce a revision, and the data
    /// that is already committed must survive.
    #[tokio::test]
    async fn a_refused_commit_keeps_the_cached_data_and_announces_no_revision() {
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        let scope_id = "synthetic-scope-synthetic-connection-0";

        // Control: the refresh commits and announces the new revision.
        let envelope = run_request(&state, Some(refresh_all_request())).await;
        let operation = only_operation(&envelope);
        assert_eq!(operation.state, OperationState::Succeeded);
        let committed = state
            .repository
            .data_revision()
            .await
            .expect("store revision");
        assert!(committed > 0, "the control refresh must commit");
        let announced = sink.events().len();
        assert_eq!(
            announced, 1,
            "the successful refresh announces one revision"
        );

        // A pending lifecycle operation makes the store refuse the next commit;
        // the fence reports the refusal as a conflict.
        state
            .repository
            .begin_operation(&quotavista_core::journal::OperationRecord {
                operation_id: "synthetic-lifecycle-1".to_owned(),
                connection_id: CONNECTION_ID.to_owned(),
                kind: quotavista_core::journal::OperationKind::Delete,
                phase: quotavista_core::journal::OperationPhase::Pending,
                candidate_ref: None,
                previous_ref: None,
                generation: 0,
            })
            .await
            .expect("the lifecycle operation must be recorded");

        let envelope = run_request(&state, Some(refresh_all_request())).await;
        let operation = only_operation(&envelope);
        assert_eq!(operation.state, OperationState::Failed);
        assert_eq!(
            operation.error.expect("a typed error").code(),
            ErrorCode::Conflict
        );

        // The committed data survives the refused commit, the store revision
        // did not move, and no revision was announced.
        let snapshot = state
            .repository
            .snapshot(scope_id, None, "tokens")
            .await
            .expect("the snapshot read must succeed");
        assert!(snapshot.is_some(), "the cached data must survive");
        assert_eq!(
            state
                .repository
                .data_revision()
                .await
                .expect("store revision"),
            committed,
            "a refused commit must not advance the store revision"
        );
        assert_eq!(
            sink.events().len(),
            announced,
            "a refused commit must not announce a revision"
        );
    }
}
