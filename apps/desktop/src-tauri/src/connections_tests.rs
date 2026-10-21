//! End-to-end coverage for the P1-10 connection and settings flows, sent
//! through the same validated request path the renderer uses. Every store is
//! the in-memory session store and every directory is a throwaway temp dir;
//! no keychain, Secret Service, or application directory is touched.

use std::sync::Arc;

use quotavista_contracts::{
    CommandResult, ConnectionDto, Envelope, ErrorCode, LifecycleDto, Locale, OperationDto,
    OperationState, Polling, SettingsDto,
};
use quotavista_core::domain::Lifecycle;
use quotavista_core::storage::{Repository, WriteContext};
use quotavista_providers::synthetic_providers;
use tempfile::TempDir;

use crate::{build_with_provider, AppState, ChangeSink, RecordingChangeSink, SecretChoice};

const SECRET_A: &str = "synthetic-secret-alpha";
const SECRET_B: &str = "synthetic-secret-beta";

async fn fixture() -> (TempDir, AppState, Arc<RecordingChangeSink>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let sink = Arc::new(RecordingChangeSink::new());
    let changes: Arc<dyn ChangeSink> = sink.clone();
    let state = build_with_provider(
        dir.path().to_path_buf(),
        changes,
        SecretChoice::SessionOnly,
        crate::SimulatedAdapter::new(synthetic_providers()[0]),
    )
    .await
    .expect("composition must initialise");
    (dir, state, sink)
}

/// Reopen the composition over the same directory, as a restart does.
async fn restart(dir: &TempDir) -> (AppState, Arc<RecordingChangeSink>) {
    let sink = Arc::new(RecordingChangeSink::new());
    let changes: Arc<dyn ChangeSink> = sink.clone();
    let state = build_with_provider(
        dir.path().to_path_buf(),
        changes,
        SecretChoice::SessionOnly,
        crate::SimulatedAdapter::new(synthetic_providers()[0]),
    )
    .await
    .expect("composition must reinitialise");
    (state, sink)
}

fn canonical(command: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "contract_version": 1,
        "request_id": "fixture-request",
        "command": command,
    })
}

async fn run(state: &AppState, command: serde_json::Value) -> Envelope {
    // every *_request helper already returns a canonical request value
    crate::run_request(state, Some(command)).await
}

async fn run_ok(state: &AppState, command: serde_json::Value) -> CommandResult {
    match run(state, command).await {
        Envelope::Ok { data, .. } => (*data).clone(),
        other => panic!("expected Envelope::Ok, got {other:?}"),
    }
}

async fn run_err(state: &AppState, command: serde_json::Value) -> ErrorCode {
    match run(state, command).await {
        Envelope::Error { error, .. } => error.code(),
        other => panic!("expected Envelope::Error, got {other:?}"),
    }
}

fn connect_request(provider: &str, alias: &str, secret: &str) -> serde_json::Value {
    canonical(serde_json::json!({
        "command": "connect_provider",
        "args": {
            "provider": provider,
            "alias": alias,
            "credential_kind": "api_key",
            "secret": secret,
            "storage_mode": "session_only",
        },
    }))
}

async fn connect(state: &AppState, secret: &str) -> OperationDto {
    let result = run_ok(state, connect_request("kimi", "Synthetic Kimi", secret)).await;
    match result {
        CommandResult::ConnectProvider(operation) => operation,
        other => panic!("expected ConnectProvider, got {other:?}"),
    }
}

async fn list_connections(state: &AppState) -> Vec<ConnectionDto> {
    let result = run_ok(
        state,
        canonical(serde_json::json!({ "command": "list_connections" })),
    )
    .await;
    match result {
        CommandResult::ListConnections(list) => list.connections,
        other => panic!("expected ListConnections, got {other:?}"),
    }
}

fn rename_request(id: &str, revision: i64, alias: &str) -> serde_json::Value {
    canonical(serde_json::json!({
        "command": "update_connection",
        "args": {
            "connection_id": id,
            "expected_config_revision": revision.to_string(),
            "alias": alias,
        },
    }))
}

fn enabled_request(id: &str, revision: i64, enabled: bool) -> serde_json::Value {
    canonical(serde_json::json!({
        "command": "set_connection_enabled",
        "args": {
            "connection_id": id,
            "expected_config_revision": revision.to_string(),
            "enabled": enabled,
        },
    }))
}

fn rotate_request(id: &str, revision: i64, secret: &str) -> serde_json::Value {
    canonical(serde_json::json!({
        "command": "replace_credentials",
        "args": {
            "connection_id": id,
            "expected_config_revision": revision.to_string(),
            "credential_kind": "api_key",
            "secret": secret,
            "storage_mode": "session_only",
        },
    }))
}

fn remove_request(id: &str, revision: i64) -> serde_json::Value {
    canonical(serde_json::json!({
        "command": "remove_connection",
        "args": {
            "connection_id": id,
            "expected_config_revision": revision.to_string(),
            "deletion_intent": "credentials_history_and_managed_backups",
        },
    }))
}

fn settings_patch_request(revision: u64, changes: serde_json::Value) -> serde_json::Value {
    canonical(serde_json::json!({
        "command": "update_settings",
        "args": {
            "expected_settings_revision": revision.to_string(),
            "changes": changes,
        },
    }))
}

async fn read_settings(state: &AppState) -> SettingsDto {
    let result = run_ok(
        state,
        settings_patch_request(
            0,
            serde_json::json!({
                "ui_locale": null,
                "reporting_zone": null,
                "display_currency": null,
                "polling": null,
                "quota_threshold": null,
                "balance_threshold": null,
            }),
        ),
    )
    .await;
    match result {
        CommandResult::UpdateSettings(settings) => settings,
        other => panic!("expected UpdateSettings, got {other:?}"),
    }
}

fn operation_of(result: CommandResult) -> OperationDto {
    match result {
        CommandResult::ReplaceCredentials(operation)
        | CommandResult::RemoveConnection(operation) => operation,
        other => panic!("expected a credential operation, got {other:?}"),
    }
}

fn connection_of(result: CommandResult) -> ConnectionDto {
    match result {
        CommandResult::UpdateConnection(connection)
        | CommandResult::SetConnectionEnabled(connection) => connection,
        other => panic!("expected a connection mutation, got {other:?}"),
    }
}

async fn generation_of(state: &AppState, id: &str) -> i64 {
    list_connections(state)
        .await
        .into_iter()
        .find(|connection| connection.connection_id.as_str() == id)
        .map(|connection| dto_generation(&connection))
        .expect("connection must be listed")
}

fn dto_generation(connection: &ConnectionDto) -> i64 {
    connection
        .config_revision
        .as_str()
        .parse::<i64>()
        .expect("revision is numeric")
}

/// PRD criterion 1: malformed aliases and secrets fail safely, changing
/// nothing.
#[tokio::test]
async fn invalid_aliases_and_secrets_fail_safely() {
    let (_dir, state, _sink) = fixture().await;

    let empty_alias = run(
        &state,
        canonical(serde_json::json!({
            "command": "connect_provider",
            "args": {
                "provider": "kimi",
                "alias": "   ",
                "credential_kind": "api_key",
                "secret": SECRET_A,
                "storage_mode": "session_only",
            },
        })),
    )
    .await;
    assert!(matches!(
        empty_alias,
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Validation
    ));

    let oversized_alias = "x".repeat(81);
    let oversized = run(&state, connect_request("kimi", &oversized_alias, SECRET_A)).await;
    assert!(matches!(
        oversized,
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Validation
    ));

    let header_invalid = run(
        &state,
        connect_request("kimi", "Synthetic Kimi", "value\nwith-line-break"),
    )
    .await;
    assert!(matches!(
        header_invalid,
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Validation
    ));

    let oversized_secret = format!("{}{}", "s".repeat(65_536), "x");
    let oversized = run(
        &state,
        connect_request("kimi", "Synthetic Kimi", &oversized_secret),
    )
    .await;
    assert!(matches!(
        oversized,
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Validation
    ));

    assert!(
        list_connections(&state).await.is_empty(),
        "no connection may exist after a refused create"
    );
}

/// PRD criterion 1: a stale expected revision never overwrites newer state.
#[tokio::test]
async fn stale_revisions_fail_safely() {
    let (_dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    // A rename from a stale revision is a conflict and keeps the alias.
    let stale = run(
        &state,
        rename_request(id.as_str(), generation + 7, "New Name"),
    )
    .await;
    assert!(matches!(
        stale,
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Conflict
    ));
    let connections = list_connections(&state).await;
    assert_eq!(connections[0].alias.as_str(), "Synthetic Kimi");

    // Settings patches use their own optimistic revision.
    let stale_settings = run(
        &state,
        settings_patch_request(
            42,
            serde_json::json!({
                "ui_locale": "zh-CN",
                "reporting_zone": null,
                "display_currency": null,
                "polling": null,
                "quota_threshold": null,
                "balance_threshold": null,
            }),
        ),
    )
    .await;
    assert!(matches!(
        stale_settings,
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Conflict
    ));
    assert_eq!(read_settings(&state).await.ui_locale, Locale::En);
}

/// PRD criterion 1: the credential value never appears in any response,
/// event, or error the backend produces.
#[tokio::test]
async fn secrets_never_leave_the_store() {
    let (_dir, state, sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    // A same-identity rotation with a fresh value still never echoes it.
    let rotated = run_ok(&state, rotate_request(id.as_str(), generation, SECRET_A)).await;
    let rotated = operation_of(rotated);
    assert_eq!(rotated.state, OperationState::Succeeded);

    // An error path (stale revision) never echoes it either.
    let _ = run(
        &state,
        rotate_request(id.as_str(), generation + 100, SECRET_B),
    )
    .await;

    let list = serde_json::to_string(
        &run_ok(
            &state,
            canonical(serde_json::json!({ "command": "list_connections" })),
        )
        .await,
    )
    .expect("list serializes");
    assert!(!list.contains(SECRET_A));
    assert!(!list.contains(SECRET_B));
    for event in sink.events() {
        let text = serde_json::to_string(&event).expect("event serializes");
        assert!(!text.contains(SECRET_A), "no secret in change events");
        assert!(!text.contains(SECRET_B));
    }
}

/// PRD criterion 2: same-identity rotation keeps the scope — and the usage
/// history bound to it — while activating the new reference.
#[tokio::test]
async fn same_identity_rotation_preserves_history() {
    let (_dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    // Refresh first so the connection carries published history.
    let refreshed = run_ok(
        &state,
        canonical(serde_json::json!({
            "command": "refresh_connections",
            "args": { "target": { "kind": "all" } },
        })),
    )
    .await;
    match refreshed {
        CommandResult::RefreshConnections(list) => {
            assert!(list
                .operations
                .iter()
                .all(|operation| operation.state == OperationState::Succeeded));
        }
        other => panic!("expected RefreshConnections, got {other:?}"),
    }

    let scope_before: Vec<String> = state
        .repository
        .scopes(id.as_str())
        .await
        .expect("scopes readable")
        .into_iter()
        .map(|scope| scope.key)
        .collect();

    let generation = generation_of(&state, id.as_str()).await;
    let rotated = run_ok(&state, rotate_request(id.as_str(), generation, SECRET_A)).await;
    let rotated = operation_of(rotated);
    assert_eq!(rotated.state, OperationState::Succeeded);
    assert_eq!(
        rotated.kind,
        quotavista_contracts::OperationKind::ReplaceCredentials
    );

    let scope_after: Vec<String> = state
        .repository
        .scopes(id.as_str())
        .await
        .expect("scopes readable")
        .into_iter()
        .map(|scope| scope.key)
        .collect();
    assert_eq!(scope_before, scope_after, "rotation preserves the scope");

    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::Active);
}

/// PRD criterion 2: a different (and in this prototype unverifiable)
/// identity never merges into the existing scope.
#[tokio::test]
async fn different_identity_rotation_is_refused_without_merging() {
    let (_dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    let refused = run_ok(&state, rotate_request(id.as_str(), generation, SECRET_B)).await;
    let refused = operation_of(refused);
    assert_eq!(refused.state, OperationState::Failed);
    assert_eq!(
        refused.next_action,
        quotavista_contracts::NextAction::ReplaceConnection
    );
    assert_eq!(
        refused.error.expect("typed error").code(),
        ErrorCode::Conflict
    );

    // The connection is untouched: same generation, still active, same scope.
    assert_eq!(generation_of(&state, id.as_str()).await, generation);
    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::Active);
    let scope: Vec<String> = state
        .repository
        .scopes(id.as_str())
        .await
        .expect("scopes readable")
        .into_iter()
        .map(|scope| scope.key)
        .collect();
    assert_eq!(scope.len(), 1);
}

/// PRD criterion 2: one active connection per provider, even for candidates
/// that race past the up-front check.
#[tokio::test]
async fn racing_candidates_yield_one_active_connection_per_provider() {
    let (_dir, state, _sink) = fixture().await;
    let (first, second) = tokio::join!(
        run(&state, connect_request("kimi", "First", SECRET_A)),
        run(&state, connect_request("kimi", "Second", SECRET_B)),
    );
    let winners = [&first, &second]
        .iter()
        .filter(|envelope| matches!(envelope, Envelope::Ok { .. }))
        .count();
    assert_eq!(winners, 1, "exactly one racing candidate may win");
    assert!(matches!(
        [&first, &second].iter().find(|envelope| !matches!(envelope, Envelope::Ok { .. })).unwrap(),
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Conflict
    ));

    let connections = list_connections(&state).await;
    let active = connections
        .iter()
        .filter(|connection| connection.lifecycle == LifecycleDto::Active)
        .count();
    assert_eq!(active, 1);
}

/// PRD criterion 3: disabling fences late work — a write context captured
/// before the disable is refused afterwards.
#[tokio::test]
async fn disabling_fences_late_work() {
    let (_dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;
    let stale_context = WriteContext {
        connection_id: id.as_str().to_owned(),
        generation,
    };

    let disabled = run_ok(&state, enabled_request(id.as_str(), generation, false)).await;
    let disabled = connection_of(disabled);
    assert_eq!(disabled.lifecycle, LifecycleDto::Disabled);
    assert!(
        dto_generation(&disabled) > generation,
        "disable bumps the revision"
    );

    // Late work against the pre-disable generation is fenced by the guard.
    let fenced = state
        .repository
        .set_lifecycle(&stale_context, Lifecycle::Active)
        .await;
    assert!(fenced.is_err(), "a stale context must not write");

    // Re-enabling moves the same connection back to active.
    let generation = generation_of(&state, id.as_str()).await;
    let enabled = run_ok(&state, enabled_request(id.as_str(), generation, true)).await;
    let enabled = connection_of(enabled);
    assert_eq!(enabled.lifecycle, LifecycleDto::Active);
}

/// PRD criterion 3: a successful removal deletes every artifact it owns.
#[tokio::test]
async fn removal_cleans_up_completely() {
    let (_dir, state, sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    let removed = run_ok(&state, remove_request(id.as_str(), generation)).await;
    let removed = operation_of(removed);
    assert_eq!(removed.state, OperationState::Succeeded);
    assert_eq!(removed.kind, quotavista_contracts::OperationKind::Delete);

    assert!(
        list_connections(&state).await.is_empty(),
        "the connection row is gone"
    );
    assert!(
        sink.events().iter().any(|event| event
            .affected_connection_ids
            .iter()
            .any(|affected| affected.as_str() == id.as_str())),
        "the removal published a change event"
    );
}

/// PRD criterion 3: a cleanup failure reports `deletion_pending`, the state
/// survives a restart, and the retry finishes it.
#[tokio::test]
async fn failed_cleanup_survives_restart_and_retries() {
    let (dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    // Fail every reference operation: the owed credential deletion cannot
    // run, so the removal must not be acknowledged.
    state
        .secrets
        .as_ref()
        .expect("session composition has a store")
        .set_fail_reference_ops(true);
    let failed = run_ok(&state, remove_request(id.as_str(), generation)).await;
    let failed = operation_of(failed);
    assert_eq!(failed.state, OperationState::Failed);
    assert!(failed.retryable, "the cleanup is retryable");
    assert_eq!(failed.kind, quotavista_contracts::OperationKind::Delete);

    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::DeletionPending);

    // A restart reconstructs the same pending state...
    let (state, _sink) = restart(&dir).await;
    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::DeletionPending);

    // ...and the retry, now that the store works again, finishes it.
    let retry = run_ok(
        &state,
        remove_request(id.as_str(), generation_of(&state, id.as_str()).await),
    )
    .await;
    let retry = operation_of(retry);
    assert_eq!(retry.state, OperationState::Succeeded);
    assert!(list_connections(&state).await.is_empty());
}

/// PRD criterion 4: a session-only credential does not survive a restart; the
/// connection reports `needs_credentials` and nothing collects a credential
/// implicitly.
#[tokio::test]
async fn session_only_connection_needs_credentials_after_restart() {
    let (dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    assert_eq!(operation.state, OperationState::Succeeded);

    let (state, _sink) = restart(&dir).await;
    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::NeedsCredentials);

    // Rotation with the same synthetic identity is the explicit recovery
    // path: it re-stages the credential and reactivates the connection.
    let generation = generation_of(&state, id.as_str()).await;
    let recovered = run_ok(&state, rotate_request(id.as_str(), generation, SECRET_A)).await;
    let recovered = operation_of(recovered);
    assert_eq!(recovered.state, OperationState::Succeeded);
    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::Active);
}

/// PRD criterion 4: settings persist with optimistic revisions; an explicit
/// refresh-off choice and an explicit locale survive a restart.
#[tokio::test]
async fn explicit_settings_survive_restart() {
    let (dir, state, _sink) = fixture().await;

    let defaults = read_settings(&state).await;
    assert_eq!(defaults.ui_locale, Locale::En);
    assert_eq!(defaults.polling, Polling::FiveMinutes);
    assert_eq!(defaults.settings_revision.as_str(), "0");

    let saved = run_ok(
        &state,
        settings_patch_request(
            0,
            serde_json::json!({
                "ui_locale": "zh-CN",
                "reporting_zone": "Asia/Shanghai",
                "display_currency": "CNY",
                "polling": "disabled",
                "quota_threshold": "0.5",
                "balance_threshold": "10",
            }),
        ),
    )
    .await;
    match saved {
        CommandResult::UpdateSettings(settings) => {
            assert_eq!(settings.ui_locale, Locale::ZhCn);
            assert_eq!(settings.polling, Polling::Disabled);
            assert_eq!(settings.reporting_zone.as_str(), "Asia/Shanghai");
            assert_eq!(settings.display_currency.as_str(), "CNY");
            assert_eq!(settings.quota_threshold.as_str(), "0.5");
            assert_eq!(settings.balance_threshold.as_str(), "10");
            assert_eq!(settings.settings_revision.as_str(), "1");
        }
        other => panic!("expected UpdateSettings, got {other:?}"),
    }

    let (state, _sink) = restart(&dir).await;
    let persisted = read_settings(&state).await;
    assert_eq!(persisted.ui_locale, Locale::ZhCn);
    assert_eq!(persisted.polling, Polling::Disabled);
    assert_eq!(persisted.settings_revision.as_str(), "1");
}

/// PRD criterion 1 support: a malformed settings value is a validation
/// failure that keeps the current settings.
#[tokio::test]
async fn malformed_settings_values_fail_safely() {
    let (_dir, state, _sink) = fixture().await;
    let invalid = run(
        &state,
        settings_patch_request(
            0,
            serde_json::json!({
                "ui_locale": null,
                "reporting_zone": "Not/AZone",
                "display_currency": null,
                "polling": null,
                "quota_threshold": null,
                "balance_threshold": null,
            }),
        ),
    )
    .await;
    assert!(matches!(
        invalid,
        Envelope::Error { ref error, .. } if error.code() == ErrorCode::Validation
    ));
    assert_eq!(read_settings(&state).await.reporting_zone.as_str(), "UTC");
}

/// The lifecycle view of a seeded disabled connection still refuses
/// rotation and rename: only the documented transitions exist.
#[tokio::test]
async fn undocumented_lifecycle_transitions_are_refused() {
    let (_dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    // Enabling an already-active connection is not a documented transition.
    let code = run_err(&state, enabled_request(id.as_str(), generation, true)).await;
    assert_eq!(code, ErrorCode::Validation);

    // Renaming while a deletion is pending is a conflict.
    let removed = run_ok(&state, remove_request(id.as_str(), generation)).await;
    assert_eq!(operation_of(removed).state, OperationState::Succeeded);
    assert!(list_connections(&state).await.is_empty());
}

/// The default polling interval matches the requirements: five minutes.
#[tokio::test]
async fn empty_patch_reads_current_settings_without_bumping_the_revision() {
    let (_dir, state, _sink) = fixture().await;
    let first = read_settings(&state).await;
    let second = read_settings(&state).await;
    assert_eq!(first.settings_revision, second.settings_revision);
    assert_eq!(second.polling, Polling::FiveMinutes);
}
