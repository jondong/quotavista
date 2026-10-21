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
use quotavista_core::secrets::SecretStore;
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
async fn run_capturing(
    state: &AppState,
    command: serde_json::Value,
    responses: &mut Vec<String>,
) -> Envelope {
    let envelope = run(state, command).await;
    responses.push(serde_json::to_string(&envelope).expect("envelope serializes"));
    envelope
}

#[tokio::test]
async fn secrets_never_leave_the_store() {
    let (_dir, state, sink) = fixture().await;
    let mut responses: Vec<String> = Vec::new();

    // Every response this flow produces is captured and scanned — the
    // connect, rename, disable, enable, rotate, refresh, and remove results,
    // the stale-revision error, and the final list read — not only the list.
    let connected = run_capturing(
        &state,
        connect_request("kimi", "Synthetic Kimi", SECRET_A),
        &mut responses,
    )
    .await;
    let id = match connected {
        Envelope::Ok { ref data, .. } => match &**data {
            CommandResult::ConnectProvider(operation) => operation
                .connection_id
                .clone()
                .expect("setup names the connection"),
            other => panic!("expected ConnectProvider, got {other:?}"),
        },
        other => panic!("expected Envelope::Ok, got {other:?}"),
    };

    let generation = generation_of(&state, id.as_str()).await;
    let _ = run_capturing(
        &state,
        rename_request(id.as_str(), generation, "Renamed"),
        &mut responses,
    )
    .await;
    let generation = generation_of(&state, id.as_str()).await;
    let _ = run_capturing(
        &state,
        enabled_request(id.as_str(), generation, false),
        &mut responses,
    )
    .await;
    let generation = generation_of(&state, id.as_str()).await;
    let _ = run_capturing(
        &state,
        enabled_request(id.as_str(), generation, true),
        &mut responses,
    )
    .await;

    // A same-identity rotation with a fresh value still never echoes it.
    let generation = generation_of(&state, id.as_str()).await;
    let rotated = run_capturing(
        &state,
        rotate_request(id.as_str(), generation, SECRET_A),
        &mut responses,
    )
    .await;
    match rotated {
        Envelope::Ok { ref data, .. } => match &**data {
            CommandResult::ReplaceCredentials(operation) => {
                assert_eq!(operation.state, OperationState::Succeeded);
            }
            other => panic!("expected ReplaceCredentials, got {other:?}"),
        },
        other => panic!("expected Envelope::Ok, got {other:?}"),
    }

    // An error path (stale revision) never echoes it either.
    let generation = generation_of(&state, id.as_str()).await;
    let _ = run_capturing(
        &state,
        rotate_request(id.as_str(), generation + 100, SECRET_B),
        &mut responses,
    )
    .await;

    let _ = run_capturing(
        &state,
        canonical(serde_json::json!({
            "command": "refresh_connections",
            "args": { "target": { "kind": "all" } },
        })),
        &mut responses,
    )
    .await;

    let generation = generation_of(&state, id.as_str()).await;
    let _ = run_capturing(
        &state,
        remove_request(id.as_str(), generation),
        &mut responses,
    )
    .await;

    let _ = run_capturing(
        &state,
        canonical(serde_json::json!({ "command": "list_connections" })),
        &mut responses,
    )
    .await;

    for (index, response) in responses.iter().enumerate() {
        assert!(
            !response.contains(SECRET_A),
            "no secret in response {index}"
        );
        assert!(
            !response.contains(SECRET_B),
            "no secret in response {index}"
        );
    }
    for event in sink.events() {
        let text = serde_json::to_string(&event).expect("event serializes");
        assert!(!text.contains(SECRET_A), "no secret in change events");
        assert!(!text.contains(SECRET_B), "no secret in change events");
    }
}

/// PRD criterion 2: same-identity rotation keeps the scope — and the usage
/// history bound to it — while activating the new reference.
#[tokio::test]
async fn same_identity_rotation_preserves_history() {
    let (_dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");

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

    // The published history bound to the scope survives the rotation: the
    // current-value projection the earlier refresh produced is still there.
    let scope_id = state
        .repository
        .scopes(id.as_str())
        .await
        .expect("scopes readable")
        .into_iter()
        .map(|scope| scope.id)
        .next()
        .expect("connection has a scope");
    let retained = state
        .repository
        .snapshot(&scope_id, None, "tokens")
        .await
        .expect("the snapshot read must succeed");
    assert!(
        retained.is_some(),
        "rotation must retain the published history"
    );

    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::Active);
}
/// Review round 1, blocking finding 1: a rotation whose superseded-reference
/// cleanup fails must keep the credential it already activated. The
/// operation stays pending cleanup behind the fence, recovery completes it,
/// and the live credential stays readable throughout.
#[tokio::test]
async fn rotation_cleanup_failure_keeps_the_activated_credential() {
    let (_dir, state, _sink) = fixture().await;
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");
    let generation = generation_of(&state, id.as_str()).await;

    // Fail reference deletion only: the candidate write and the activation
    // still commit, then the superseded-reference cleanup fails.
    state
        .secrets
        .as_ref()
        .expect("session composition has a store")
        .set_fail_reference_deletes(true);
    let failed = run(&state, rotate_request(id.as_str(), generation, SECRET_A)).await;
    assert!(
        matches!(
            &failed,
            Envelope::Error { ref error, .. } if error.code() == ErrorCode::StorageError
        ),
        "the pending cleanup reports a retryable storage failure, got {failed:?}"
    );

    // The activated candidate is the live credential: readable, with the
    // connection still active and the operation visibly pending cleanup.
    let records = state
        .repository
        .operations(Some(id.as_str()))
        .await
        .expect("operations readable");
    let pending = records
        .iter()
        .find(|record| record.kind == quotavista_core::journal::OperationKind::ReplaceCredentials)
        .expect("the rotation stays pending");
    let current = state
        .repository
        .credential_ref(id.as_str())
        .await
        .expect("credential ref readable")
        .expect("the activated candidate is live");
    assert_eq!(
        current,
        pending
            .candidate_ref
            .clone()
            .expect("the record names its candidate")
    );
    let superseded = pending
        .previous_ref
        .clone()
        .expect("the rotation names the superseded reference");
    assert_ne!(current, superseded);
    let store = state.secrets.as_ref().expect("store present");
    assert_eq!(
        store
            .read(&current)
            .expect("the activated credential stays readable"),
        SECRET_A
    );
    assert_eq!(
        store
            .read(&superseded)
            .expect("the superseded credential is held until recovery"),
        SECRET_A
    );
    let connections = list_connections(&state).await;
    assert_eq!(connections[0].lifecycle, LifecycleDto::Active);

    // Recovery — the same resumption a restart runs — completes the owed
    // cleanup and keeps the live credential.
    store.set_fail_reference_deletes(false);
    let coordinator = state.coordinator.as_ref().expect("coordinator present");
    coordinator
        .lock()
        .await
        .recover()
        .await
        .expect("recovery completes the pending cleanup");
    assert_eq!(
        store
            .read(&current)
            .expect("the activated credential survives recovery"),
        SECRET_A
    );
    assert!(
        store.read(&superseded).is_err(),
        "recovery removed the superseded reference"
    );
    let records = state
        .repository
        .operations(Some(id.as_str()))
        .await
        .expect("operations readable");
    assert!(
        records.iter().all(
            |record| record.kind != quotavista_core::journal::OperationKind::ReplaceCredentials
        ),
        "the rotation projection completed"
    );

    // The fence is released: the connection accepts a later mutation.
    let generation = generation_of(&state, id.as_str()).await;
    let disabled = run_ok(&state, enabled_request(id.as_str(), generation, false)).await;
    assert_eq!(connection_of(disabled).lifecycle, LifecycleDto::Disabled);
}

/// Re-review round 2: a rotation refused *before any journal intent* — here
/// the operation fence is still held by the pending superseded-reference
/// cleanup — must remove its staged candidate. A rejected request may not
/// retain a copy of the submitted secret until process exit. The committed
/// candidate is never touched on that path; that case is covered by
/// `rotation_cleanup_failure_keeps_the_activated_credential` above.
#[tokio::test]
async fn pre_journal_rejection_cleans_the_staged_candidate() {
    let (_dir, state, _sink) = fixture().await;
    let store = state.secrets.as_ref().expect("store present");
    let operation = connect(&state, SECRET_A).await;
    let id = operation.connection_id.expect("setup names the connection");

    // A committed rotation whose superseded-reference cleanup fails: the
    // candidate is live, the cleanup stays pending behind the fence.
    store.set_fail_reference_deletes(true);
    let generation = generation_of(&state, id.as_str()).await;
    let failed = run(&state, rotate_request(id.as_str(), generation, SECRET_A)).await;
    assert!(
        matches!(
            &failed,
            Envelope::Error { ref error, .. } if error.code() == ErrorCode::StorageError
        ),
        "the pending cleanup reports a retryable storage failure, got {failed:?}"
    );
    assert_eq!(
        store.session_staged_count(),
        0,
        "the committed rotation left nothing staged"
    );

    // While the fence is held (it outlives the injected store failure),
    // another rotation with the CURRENT revision is refused before any
    // journal intent. Store availability is restored first: the refusal
    // itself must remove the staged copy while the store is healthy.
    store.set_fail_reference_deletes(false);
    // The committed rotation bumped the generation even though its cleanup
    // is pending; re-read it so the request passes the revision guard and
    // actually reaches the coordinator's fence refusal.
    let generation = generation_of(&state, id.as_str()).await;
    let refused = run(&state, rotate_request(id.as_str(), generation, SECRET_A)).await;
    assert!(
        matches!(
            &refused,
            Envelope::Error { ref error, .. } if error.code() == ErrorCode::Conflict
        ),
        "the fenced rotation is refused with a conflict, got {refused:?}"
    );
    assert_eq!(
        store.session_staged_count(),
        0,
        "the refused rotation leaves no staged copy of the secret"
    );
    let records = state
        .repository
        .operations(Some(id.as_str()))
        .await
        .expect("operations readable");
    assert_eq!(
        records
            .iter()
            .filter(
                |record| record.kind == quotavista_core::journal::OperationKind::ReplaceCredentials
            )
            .count(),
        1,
        "the refusal journaled no second rotation"
    );

    // The live credential is untouched by the refusal, and recovery still
    // completes the owed cleanup.
    let current = state
        .repository
        .credential_ref(id.as_str())
        .await
        .expect("credential ref readable")
        .expect("the live reference is present");
    assert_eq!(
        store
            .read(&current)
            .expect("the live credential stays readable"),
        SECRET_A
    );
    store.set_fail_reference_deletes(false);
    state
        .coordinator
        .as_ref()
        .expect("coordinator present")
        .lock()
        .await
        .recover()
        .await
        .expect("recovery completes the pending cleanup");
    assert_eq!(
        store
            .read(&current)
            .expect("the live credential survives recovery"),
        SECRET_A
    );
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
    // The refused candidate must not leave a ghost row behind: exactly one
    // connection — the winner — remains, with no lingering PendingSetup.
    assert_eq!(
        connections.len(),
        1,
        "the refused racing setup leaves no ghost connection"
    );
    assert!(connections
        .iter()
        .all(|connection| connection.lifecycle != LifecycleDto::PendingSetup));
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
    assert_eq!(defaults.polling, Polling::Disabled);
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

/// The never-saved seed keeps the shipped Off default (P1-11 semantics on
/// main); an explicit interval choice — including Off — is what survives a
/// restart, and that is covered by `explicit_settings_survive_restart`.
#[tokio::test]
async fn empty_patch_reads_current_settings_without_bumping_the_revision() {
    let (_dir, state, _sink) = fixture().await;
    let first = read_settings(&state).await;
    let second = read_settings(&state).await;
    assert_eq!(first.settings_revision, second.settings_revision);
    assert_eq!(second.polling, Polling::Disabled);
}

/// The full refresh-interval cycle Off -> 1 min -> 5 min -> 15 min -> Off
/// persists across restarts: after every save the composition is closed and
/// rebuilt over the same directory, the stored interval and the settings
/// revision survive, and wakes stay blocked whenever the interval is Off.
#[tokio::test]
async fn refresh_interval_cycle_survives_restart_and_blocks_wakes_when_off() {
    let (dir, mut state, _sink) = fixture().await;
    let mut revision: u64 = read_settings(&state)
        .await
        .settings_revision
        .as_str()
        .parse()
        .expect("settings revision is numeric");
    assert_eq!(read_settings(&state).await.polling, Polling::Disabled);
    assert_wake_blocked_when_off(&state).await;

    for polling in [
        Polling::OneMinute,
        Polling::FiveMinutes,
        Polling::FifteenMinutes,
        Polling::Disabled,
    ] {
        let saved = match run_ok(
            &state,
            settings_patch_request(
                revision,
                serde_json::json!({
                    "ui_locale": null,
                    "reporting_zone": null,
                    "display_currency": null,
                    "polling": polling,
                    "quota_threshold": null,
                    "balance_threshold": null,
                }),
            ),
        )
        .await
        {
            CommandResult::UpdateSettings(settings) => settings,
            other => panic!("expected UpdateSettings, got {other:?}"),
        };
        revision += 1;
        assert_eq!(saved.polling, polling);
        assert_eq!(saved.settings_revision.as_str(), revision.to_string());

        // Close SQLite (drop the whole composition) and rebuild over the
        // same directory, as a restart does.
        drop(state);
        let (reopened, _reopened_sink) = restart(&dir).await;
        let settings = read_settings(&reopened).await;
        assert_eq!(settings.polling, polling);
        assert_eq!(settings.settings_revision.as_str(), revision.to_string());
        if polling == Polling::Disabled {
            assert_wake_blocked_when_off(&reopened).await;
        }
        state = reopened;
    }
}

/// With the interval Off a wake must return before the coordinator is
/// touched: not permitted, nothing launched, nothing refused.
async fn assert_wake_blocked_when_off(state: &AppState) {
    let report = crate::lifecycle::handle_wake(state, crate::lifecycle::WakeSource::WindowFocused)
        .await
        .expect("wake report");
    assert!(!report.permitted, "wakes stay blocked while refresh is Off");
    assert!(report.launched.is_empty());
    assert_eq!(report.refused, 0);
}
