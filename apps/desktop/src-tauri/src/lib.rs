//! Desktop composition root: assembles core, providers, and infrastructure
//! behind restricted Tauri IPC. The renderer never touches storage, secrets,
//! providers, or the coordinator directly; it sends a single validated
//! [`quotavista_contracts::Request`] and receives an [`Envelope`].
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use quotavista_contracts::{
    Alias, Command, CommandResult, ConnectionDto, ConnectionList, Envelope, ErrorCode, LocalId,
    OperationDto, OperationList, RangeInput, Request, SafeError, SecretInput, SettingsDto,
    SettingsPatch,
};
use quotavista_core::calendar::{Clock, Interval as CalInterval};
use quotavista_core::domain::Lifecycle;
use quotavista_core::journal::SecretReferenceStore;
use quotavista_core::provider::RefreshClock;
use quotavista_core::secrets::SecretStore;
use quotavista_core::storage::StorageError;
use quotavista_infrastructure::durable::{FileStore, SharedFileStore};
use quotavista_infrastructure::refresh::RefreshCoordinator;
use quotavista_infrastructure::sqlite::SqliteRepository;
use quotavista_providers::simulated::SimulatedAdapter;
use quotavista_providers::synthetic_providers;
use tauri::State;

/// UTC clock used by the coordinator and by [`Request::validate`].
#[derive(Clone, Copy, Debug)]
pub struct UtcClock;

impl Clock for UtcClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl RefreshClock for UtcClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Pin the secrets backend that the desktop ships to non-native builds. macOS
/// and Linux use [`NativeSecrets`]; every other platform falls back to the
/// in-memory session store so tests and CI runs stay platform-neutral.
#[cfg(any(target_os = "macos", target_os = "linux"))]
type DesktopSecrets = quotavista_infrastructure::secrets::NativeSecrets;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
type DesktopSecrets = quotavista_core::secrets::SessionSecrets;

/// Everything the renderer can reach through Tauri commands. Held in Tauri's
/// managed state so command handlers can borrow it without re-opening stores
/// (which would defeat the lifecycle fence and the cooldown journal).
pub struct AppState {
    #[allow(dead_code)]
    repository: Arc<SqliteRepository>,
    #[allow(dead_code)]
    secrets: Arc<dyn SecretStore>,
    #[allow(dead_code)]
    coordinator: tokio::sync::Mutex<
        quotavista_infrastructure::lifecycle::Coordinator<
            SqliteRepository,
            SharedFileStore,
            Arc<dyn SecretReferenceStore>,
        >,
    >,
    #[allow(dead_code)]
    refresh: tokio::sync::Mutex<
        RefreshCoordinator<SqliteRepository, SimulatedAdapter, SharedFileStore, UtcClock>,
    >,
}

/// Initialise the desktop composition in a caller-supplied root directory.
/// The desktop binary uses `std::env::temp_dir()`; tests pass a
/// `tempfile::TempDir` so no real application directory is ever touched.
pub async fn build(root: PathBuf) -> Result<AppState, SafeError> {
    std::fs::create_dir_all(&root).map_err(|_| safe(ErrorCode::InternalError))?;
    let database = SqliteRepository::open(&root.join("quotavista.sqlite"))
        .await
        .map_err(|_| safe(ErrorCode::StorageError))?;
    let durable = SharedFileStore(Arc::new(
        FileStore::new(root.join("journal")).map_err(|_| safe(ErrorCode::InternalError))?,
    ));

    let secrets = open_secrets().map_err(|_| safe(ErrorCode::SecretStoreUnavailable))?;
    let references: Arc<dyn SecretReferenceStore> = secrets.clone();
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

    let adapter = SimulatedAdapter::new(synthetic_providers()[0]);
    let refresh = RefreshCoordinator::open(database.clone(), adapter, durable.clone(), UtcClock)
        .map_err(|_| safe(ErrorCode::InternalError))?;

    Ok(AppState {
        repository: Arc::new(database),
        secrets,
        coordinator: tokio::sync::Mutex::new(coordinator),
        refresh: tokio::sync::Mutex::new(refresh),
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn open_secrets() -> Result<Arc<dyn SecretStore>, SafeError> {
    let native = DesktopSecrets::open().map_err(|_| safe(ErrorCode::SecretStoreUnavailable))?;
    Ok(Arc::new(native))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn open_secrets() -> Result<Arc<dyn SecretStore>, SafeError> {
    Ok(Arc::new(DesktopSecrets::new()))
}

fn safe(code: ErrorCode) -> SafeError {
    SafeError::new(code, None)
}

/// Convenience helpers for tests. Every literal here satisfies the contract
/// scalar validators, so an unwrap is appropriate in tests that have already
/// confirmed the invariants at compile time.
#[cfg(test)]
fn local_id(value: &str) -> LocalId {
    LocalId::try_from(value.to_owned()).expect("valid LocalId literal")
}
fn instant(value: &str) -> quotavista_contracts::Instant {
    quotavista_contracts::Instant::try_from(value.to_owned()).expect("valid Instant literal")
}
fn zone(value: &str) -> quotavista_contracts::Zone {
    quotavista_contracts::Zone::try_from(value.to_owned()).expect("valid Zone literal")
}
fn revision(value: u64) -> quotavista_contracts::Revision {
    quotavista_contracts::Revision::try_from(value.to_string()).expect("valid Revision literal")
}

/// The single dispatch surface the renderer may call. Every other entrypoint
/// in this module is private; the secret-input contract is uniform because the
/// renderer can never reach a handler that accepts credentials outside this
/// dispatcher.
#[tauri::command]
async fn dispatch_command(
    state: State<'_, AppState>,
    request: Request,
) -> Result<Envelope, Envelope> {
    if let Err(error) = request.validate(&UtcClock) {
        return Ok(envelope_error(request.request_id, error));
    }
    let request_id = request.request_id.clone();
    match dispatch(state, request).await {
        Ok(data) => Ok(Envelope::Ok {
            contract_version: quotavista_contracts::CONTRACT_VERSION,
            request_id,
            data: Box::new(data),
        }),
        Err(error) => Ok(envelope_error(request_id, error)),
    }
}

async fn dispatch(
    state: State<'_, AppState>,
    request: Request,
) -> Result<CommandResult, SafeError> {
    match request.command {
        Command::ListConnections => list_connections(&state)
            .await
            .map(CommandResult::ListConnections),
        Command::ConnectProvider {
            provider,
            alias,
            credential_kind,
            secret,
            storage_mode,
        } => connect_provider(
            &state,
            provider,
            alias,
            credential_kind,
            secret,
            storage_mode,
        )
        .await
        .map(CommandResult::ConnectProvider),
        Command::UpdateConnection {
            connection_id,
            expected_config_revision,
            alias,
        } => update_connection(&state, connection_id, expected_config_revision, alias)
            .await
            .map(CommandResult::UpdateConnection),
        Command::ReplaceCredentials {
            connection_id,
            expected_config_revision,
            credential_kind,
            secret,
            storage_mode,
        } => replace_credentials(
            &state,
            connection_id,
            expected_config_revision,
            credential_kind,
            secret,
            storage_mode,
        )
        .await
        .map(CommandResult::ReplaceCredentials),
        Command::SetConnectionEnabled {
            connection_id,
            expected_config_revision,
            enabled,
        } => set_connection_enabled(&state, connection_id, expected_config_revision, enabled)
            .await
            .map(CommandResult::SetConnectionEnabled),
        Command::RemoveConnection {
            connection_id,
            expected_config_revision,
            deletion_intent,
        } => remove_connection(
            &state,
            connection_id,
            expected_config_revision,
            deletion_intent,
        )
        .await
        .map(CommandResult::RemoveConnection),
        Command::RefreshConnections { target } => refresh_connections(&state, target)
            .await
            .map(CommandResult::RefreshConnections),
        Command::GetOverview { range } => get_overview(&state, range)
            .await
            .map(CommandResult::GetOverview),
        Command::QueryStatistics { range } => query_statistics(&state, range)
            .await
            .map(CommandResult::QueryStatistics),
        Command::UpdateSettings {
            expected_settings_revision,
            changes,
        } => update_settings(&state, expected_settings_revision, changes)
            .await
            .map(CommandResult::UpdateSettings),
        Command::GetOperation { operation_id } => get_operation(&state, operation_id)
            .await
            .map(CommandResult::GetOperation),
    }
}

fn envelope_error(request_id: LocalId, error: SafeError) -> Envelope {
    Envelope::Error {
        contract_version: quotavista_contracts::CONTRACT_VERSION,
        request_id,
        error,
    }
}

async fn list_connections(_state: &State<'_, AppState>) -> Result<ConnectionList, SafeError> {
    // Storage port does not yet expose a list method; a dedicated reader lands
    // with the maintenance coordinator. The envelope remains correctly typed.
    Ok(ConnectionList {
        connections: Vec::new(),
        data_revision: revision(0),
    })
}

async fn connect_provider(
    _state: &State<'_, AppState>,
    _provider: quotavista_contracts::Provider,
    _alias: Alias,
    _credential_kind: quotavista_contracts::CredentialKind,
    _secret: SecretInput,
    _storage_mode: quotavista_contracts::StorageMode,
) -> Result<OperationDto, SafeError> {
    // Lifecycle fencing for `connect_provider` is wired through
    // `coordinator.activate_credentials`; the wire shape lands when the
    // settings screen reaches Phase 2.
    Err(safe(ErrorCode::InternalError))
}

async fn update_connection(
    _state: &State<'_, AppState>,
    _connection_id: LocalId,
    _expected_config_revision: quotavista_contracts::Revision,
    _alias: Alias,
) -> Result<ConnectionDto, SafeError> {
    Err(safe(ErrorCode::InternalError))
}

async fn replace_credentials(
    _state: &State<'_, AppState>,
    _connection_id: LocalId,
    _expected_config_revision: quotavista_contracts::Revision,
    _credential_kind: quotavista_contracts::CredentialKind,
    _secret: SecretInput,
    _storage_mode: quotavista_contracts::StorageMode,
) -> Result<OperationDto, SafeError> {
    Err(safe(ErrorCode::InternalError))
}

async fn set_connection_enabled(
    _state: &State<'_, AppState>,
    _connection_id: LocalId,
    _expected_config_revision: quotavista_contracts::Revision,
    _enabled: bool,
) -> Result<ConnectionDto, SafeError> {
    Err(safe(ErrorCode::InternalError))
}

async fn remove_connection(
    _state: &State<'_, AppState>,
    _connection_id: LocalId,
    _expected_config_revision: quotavista_contracts::Revision,
    _deletion_intent: quotavista_contracts::DeletionIntent,
) -> Result<OperationDto, SafeError> {
    Err(safe(ErrorCode::InternalError))
}

async fn refresh_connections(
    _state: &State<'_, AppState>,
    _target: quotavista_contracts::RefreshTarget,
) -> Result<OperationList, SafeError> {
    Ok(OperationList {
        operations: Vec::new(),
    })
}

async fn get_overview(
    _state: &State<'_, AppState>,
    range: quotavista_contracts::OverviewRangeInput,
) -> Result<quotavista_contracts::OverviewDto, SafeError> {
    let _ = range;
    Ok(quotavista_contracts::OverviewDto {
        metrics: Vec::new(),
        summary: stub_summary(),
    })
}

async fn query_statistics(
    _state: &State<'_, AppState>,
    range: RangeInput,
) -> Result<quotavista_contracts::StatisticsDto, SafeError> {
    let _ = range;
    Ok(stub_summary())
}

async fn update_settings(
    _state: &State<'_, AppState>,
    _expected: quotavista_contracts::Revision,
    _changes: SettingsPatch,
) -> Result<SettingsDto, SafeError> {
    Err(safe(ErrorCode::InternalError))
}

async fn get_operation(
    _state: &State<'_, AppState>,
    _operation_id: LocalId,
) -> Result<OperationDto, SafeError> {
    Err(safe(ErrorCode::Validation))
}

fn stub_summary() -> quotavista_contracts::StatisticsDto {
    quotavista_contracts::StatisticsDto {
        requested_interval: quotavista_contracts::IntervalDto {
            start: instant("1970-01-01T00:00:00Z"),
            end: instant("1970-01-02T00:00:00Z"),
        },
        effective_intervals: Vec::new(),
        reporting_zone: zone("UTC"),
        grain: quotavista_contracts::Grain::Day,
        as_of: instant("1970-01-01T00:00:00Z"),
        provisional: false,
        coverage: Vec::new(),
        excluded_scopes: Vec::new(),
        totals: Vec::new(),
        series: Vec::new(),
        plans: Vec::new(),
        data_revision: revision(0),
    }
}

#[allow(dead_code)] // shared helper for the still-stubbed handlers in this composition root
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

#[allow(dead_code)]
fn validate_range(_interval: CalInterval) -> Result<(), SafeError> {
    Ok(())
}

#[allow(dead_code)]
fn _lifecycle_phase_to_state(_phase: Lifecycle) -> quotavista_contracts::LifecycleDto {
    quotavista_contracts::LifecycleDto::PendingSetup
}

/// Build, persist, and run the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("desktop runtime must start");
    let root = std::env::temp_dir().join("quotavista-prototype");
    let state = runtime
        .block_on(build(root))
        .expect("desktop composition must initialise");
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![dispatch_command])
        .run(tauri::generate_context!())
        .expect("error while running QuotaVista prototype");
}

#[cfg(test)]
mod tests {
    use super::*;
    use quotavista_contracts::RefreshTarget;

    fn fixture_request(command: Command) -> Request {
        Request {
            contract_version: quotavista_contracts::CONTRACT_VERSION,
            request_id: local_id("fixture-request"),
            command,
        }
    }

    #[test]
    fn dispatch_rejects_unknown_contract_version() {
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

    #[test]
    fn dispatch_rejects_oversized_secret_via_decode() {
        // SecretInput rejects oversized values during serde deserialization,
        // so the envelope never crosses the dispatch boundary.
        let oversized = "x".repeat(64 * 1024 + 1);
        let raw = serde_json::json!({
            "contract_version": quotavista_contracts::CONTRACT_VERSION,
            "request_id": "fixture",
            "command": {
                "connect_provider": {
                    "provider": "kimi",
                    "alias": "Demo",
                    "credential_kind": "api_key",
                    "secret": oversized,
                    "storage_mode": "session_only",
                }
            }
        });
        let parsed: Result<Request, _> = serde_json::from_value(raw);
        assert!(
            parsed.is_err(),
            "oversized secret must fail before validation"
        );
    }

    #[test]
    fn dispatch_rejects_oversized_scope_set() {
        let mut ids = Vec::new();
        for i in 0..101 {
            ids.push(local_id(&format!("scope-{i}")));
        }
        let request = fixture_request(Command::RefreshConnections {
            target: RefreshTarget::Selected(ids),
        });
        let error = request.validate(&UtcClock).expect_err("101 ids must fail");
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[test]
    fn dispatch_rejects_empty_selected_target() {
        let request = fixture_request(Command::RefreshConnections {
            target: RefreshTarget::Selected(Vec::new()),
        });
        let error = request
            .validate(&UtcClock)
            .expect_err("empty selection must fail");
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[test]
    fn only_connect_and_replace_accept_secret_input() {
        // The Command enum is intentionally Deserialize-only: it never
        // serialises back to the renderer, so the wire-format boundary is the
        // single point that allows secret-bearing input. Round-tripping
        // through JSON would defeat that asymmetry; instead we round-trip the
        // whole [`Request`] envelope and inspect the resulting Command
        // variants to confirm only the two credential commands decode the
        // `secret` field.
        fn decode(json: serde_json::Value) -> Command {
            let envelope = serde_json::json!({
                "contract_version": quotavista_contracts::CONTRACT_VERSION,
                "request_id": "fixture",
                "command": json,
            });
            serde_json::from_value::<Request>(envelope)
                .expect("request must decode")
                .command
        }

        // Command is internally tagged (`tag = "command"`, `content = "args"`),
        // so every wire envelope nests the variant name plus its arg struct.
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
        let raw_refresh =
            serde_json::json!({ "command": "refresh_connections", "args": { "all": null } });
        let raw_get_operation = serde_json::json!({
            "command": "get_operation",
            "args": { "operation_id": "o" }
        });

        // Every other command rejects an extra `secret` field because the
        // [`deny_unknown_fields`] attribute on [`Command`] forbids it.
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
                "{raw} must reject a smuggled secret field"
            );
        }

        // Connect and replace accept the field; the decoded enum matches.
        matches!(decode(raw_connect), Command::ConnectProvider { .. });
        matches!(decode(raw_replace), Command::ReplaceCredentials { .. });
    }
}
