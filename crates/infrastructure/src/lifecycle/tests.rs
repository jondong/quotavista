//! Synthetic recovery harness for the lifecycle coordinator. Every database,
//! journal file, and secret reference here is throwaway: no real application data
//! directory, native secret store, or provider account is touched.
use super::*;
use crate::sqlite::SqliteRepository;
use chrono::{DateTime, Utc};
use quotavista_core::{
    domain::{Provenance, Value},
    journal::SecretStoreError,
    number::TokenTotal,
    storage::{Connection, Repository, Scope, Snapshot, StorageError, WriteContext},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tempfile::TempDir;

const CONNECTION: &str = "synthetic-connection";
const SCOPE: &str = "synthetic-scope";
const OTHER: &str = "synthetic-other";
const OTHER_SCOPE: &str = "synthetic-other-scope";
const OPERATION: &str = "synthetic-operation";
/// Opaque references only. The coordinator never receives a credential value.
const PREVIOUS: &str = "synthetic-reference-a";
const CANDIDATE: &str = "synthetic-reference-b";

type TestCoordinator = Coordinator<SqliteRepository, FakeDurable, FakeSecrets>;

/// Stand-in for the P1-05 native secret store. It keeps a synthetic value per
/// opaque reference and can refuse, so interruption points are testable without a
/// keychain.
#[derive(Clone, Debug, Default)]
struct FakeSecrets {
    state: Arc<Mutex<SecretState>>,
}

#[derive(Debug, Default)]
struct SecretState {
    present: BTreeMap<String, String>,
    deleted: Vec<String>,
    refuse_writes: bool,
    refused: BTreeSet<String>,
}

impl FakeSecrets {
    fn refuse_writes(&self) {
        self.state.lock().unwrap().refuse_writes = true;
    }

    fn refuse_delete(&self, reference: &str) {
        self.state.lock().unwrap().refused.insert(reference.into());
    }

    fn allow_delete(&self, reference: &str) {
        self.state.lock().unwrap().refused.remove(reference);
    }

    /// Pretend a reference already exists, as a restart would find it.
    fn seed(&self, reference: &str) {
        self.state
            .lock()
            .unwrap()
            .present
            .insert(reference.into(), value_for(reference));
    }

    fn present(&self) -> Vec<String> {
        self.state.lock().unwrap().present.keys().cloned().collect()
    }

    fn deleted(&self) -> Vec<String> {
        self.state.lock().unwrap().deleted.clone()
    }
}

/// Synthetic value a store would hold for a reference. Never journaled.
fn value_for(reference: &str) -> String {
    format!("synthetic-secret-value-{reference}")
}

impl SecretReferenceStore for FakeSecrets {
    fn write_candidate(&self, reference: &str) -> Result<(), SecretStoreError> {
        let mut state = self.state.lock().unwrap();
        if state.refuse_writes {
            return Err(SecretStoreError::Locked);
        }
        state.present.insert(reference.into(), value_for(reference));
        Ok(())
    }

    fn delete_reference(&self, reference: &str) -> Result<(), SecretStoreError> {
        let mut state = self.state.lock().unwrap();
        if state.refused.contains(reference) {
            return Err(SecretStoreError::Locked);
        }
        // Deleting an absent reference succeeds, so recovery resumes idempotently.
        state.present.remove(reference);
        state.deleted.push(reference.into());
        Ok(())
    }
}

/// In-memory durable store that can refuse from the Nth replacement on, so an
/// interruption lands exactly on a journal or ledger write.
#[derive(Clone, Debug, Default)]
struct FakeDurable {
    state: Arc<Mutex<DurableState>>,
}

#[derive(Debug, Default)]
struct DurableState {
    entries: BTreeMap<String, String>,
    remaining: Option<usize>,
    writes: usize,
}

impl FakeDurable {
    /// Allow `writes` more successful replacements, then fail with
    /// [`JournalError::Durability`].
    fn fail_after(&self, writes: usize) {
        self.state.lock().unwrap().remaining = Some(writes);
    }

    fn writes(&self) -> usize {
        self.state.lock().unwrap().writes
    }

    fn entry(&self, name: &str) -> Option<String> {
        self.state.lock().unwrap().entries.get(name).cloned()
    }

    /// Write external state directly, as a crash would leave it behind.
    fn put(&self, name: &str, text: &str) {
        self.state
            .lock()
            .unwrap()
            .entries
            .insert(name.into(), text.into());
    }
}

impl DurableStore for FakeDurable {
    fn read(&self, name: &str) -> Result<Option<String>, JournalError> {
        Ok(self.state.lock().unwrap().entries.get(name).cloned())
    }

    fn replace(&self, name: &str, contents: &str) -> Result<(), JournalError> {
        let mut state = self.state.lock().unwrap();
        if let Some(remaining) = state.remaining {
            if remaining == 0 {
                return Err(JournalError::Durability);
            }
            state.remaining = Some(remaining - 1);
        }
        state.writes += 1;
        state.entries.insert(name.into(), contents.into());
        Ok(())
    }
}

struct Harness {
    _dir: TempDir,
    repo: SqliteRepository,
    durable: FakeDurable,
    secrets: FakeSecrets,
}

impl Harness {
    /// Open a coordinator over the current external state without recovering.
    fn coordinator(&self) -> TestCoordinator {
        Coordinator::open(
            self.repo.clone(),
            self.durable.clone(),
            self.secrets.clone(),
        )
        .unwrap()
    }

    /// Opening must fail with `expected`: corrupt external state may never yield a
    /// coordinator that could schedule or mutate.
    fn open_err(&self, expected: JournalError) {
        match Coordinator::open(
            self.repo.clone(),
            self.durable.clone(),
            self.secrets.clone(),
        ) {
            Ok(_) => panic!("corrupt external state must not open a coordinator"),
            Err(error) => assert_eq!(error, expected),
        }
    }

    /// Open and recover, the way startup must.
    async fn recovered(&self) -> TestCoordinator {
        let mut coordinator = self.coordinator();
        coordinator.recover().await.unwrap();
        coordinator
    }
}

/// Throwaway store holding one synthetic connection.
async fn fixture() -> Harness {
    fixture_with(Lifecycle::Active).await
}

async fn fixture_with(lifecycle: Lifecycle) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let repo = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    repo.create_connection(
        &connection(CONNECTION, lifecycle),
        &scope(SCOPE, CONNECTION),
    )
    .await
    .unwrap();
    Harness {
        _dir: dir,
        repo,
        durable: FakeDurable::default(),
        secrets: FakeSecrets::default(),
    }
}

async fn active(harness: &Harness, id: &str, scope_id: &str) {
    harness
        .repo
        .create_connection(&connection(id, Lifecycle::Active), &scope(scope_id, id))
        .await
        .unwrap();
}

fn at(hour: u32) -> DateTime<Utc> {
    format!("2026-01-01T{hour:02}:00:00Z").parse().unwrap()
}

fn connection(id: &str, lifecycle: Lifecycle) -> Connection {
    Connection {
        id: id.into(),
        // Distinct per connection: the schema allows one active row per provider.
        provider: format!("synthetic-provider-{id}"),
        alias: format!("Synthetic {id}"),
        generation: 0,
        lifecycle,
    }
}

fn scope(id: &str, connection_id: &str) -> Scope {
    Scope {
        id: id.into(),
        connection_id: connection_id.into(),
        key: format!("synthetic-account-{id}"),
        kind: "account".into(),
    }
}

fn context(generation: i64) -> WriteContext {
    WriteContext {
        connection_id: CONNECTION.into(),
        generation,
    }
}

fn other_context(generation: i64) -> WriteContext {
    WriteContext {
        connection_id: OTHER.into(),
        generation,
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        scope_id: SCOPE.into(),
        product_id: None,
        metric: "synthetic-value".into(),
        value: Value::Tokens(TokenTotal(7u64.into())),
        provenance: Provenance::Synthetic,
        observed_at: at(1),
        source_as_of: Some(at(0)),
    }
}

fn manifest(included_ids: &[&str]) -> BackupManifest {
    BackupManifest {
        backup_id: "synthetic-backup".into(),
        schema_version: 1,
        created_at: at(0),
        included_ids: included_ids.iter().map(|id| (*id).to_owned()).collect(),
    }
}

fn record(
    operation_id: &str,
    kind: OperationKind,
    phase: OperationPhase,
    candidate_ref: Option<&str>,
) -> OperationRecord {
    OperationRecord {
        operation_id: operation_id.into(),
        connection_id: CONNECTION.into(),
        kind,
        phase,
        candidate_ref: candidate_ref.map(str::to_owned),
        previous_ref: None,
        generation: 0,
    }
}

async fn setup(coordinator: &mut TestCoordinator, candidate: &str) -> i64 {
    coordinator
        .activate_credentials(OPERATION, &context(0), OperationKind::Setup, candidate)
        .await
        .unwrap()
}

#[tokio::test]
async fn setup_activation_commits_and_releases_the_fence() {
    let harness = fixture_with(Lifecycle::PendingSetup).await;
    let mut coordinator = harness.recovered().await;
    let generation = setup(&mut coordinator, CANDIDATE).await;

    // Activation moved the connection to active with a new generation, the
    // projection is complete, and the external journal holds no fence.
    assert_eq!(generation, 1);
    let row = harness.repo.connection(CONNECTION).await.unwrap().unwrap();
    assert_eq!(row.lifecycle, Lifecycle::Active);
    assert_eq!(row.generation, 1);
    assert_eq!(
        harness
            .repo
            .credential_ref(CONNECTION)
            .await
            .unwrap()
            .as_deref(),
        Some(CANDIDATE)
    );
    assert!(coordinator.journal().is_empty());
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
    assert_eq!(harness.secrets.present(), vec![CANDIDATE]);
    assert_eq!(
        harness.durable.entry(JOURNAL_NAME).unwrap(),
        Journal::default().encode()
    );
    assert_eq!(
        coordinator
            .schedule_context(CONNECTION)
            .await
            .unwrap()
            .generation,
        1
    );
}

#[tokio::test]
async fn durability_failure_before_the_journal_write_stops_the_mutation() {
    let harness = fixture().await;
    harness.durable.fail_after(0);
    let mut coordinator = harness.recovered().await;

    let error = coordinator
        .activate_credentials(OPERATION, &context(0), OperationKind::Setup, CANDIDATE)
        .await
        .unwrap_err();

    // Durability was not established, so nothing else happened: no secret, no
    // projection, no lifecycle change, and no acknowledgement.
    assert_eq!(error, JournalError::Durability);
    assert_eq!(harness.durable.writes(), 0);
    assert!(harness.secrets.present().is_empty());
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
    assert_eq!(harness.repo.credential_ref(CONNECTION).await.unwrap(), None);
    let row = harness.repo.connection(CONNECTION).await.unwrap().unwrap();
    assert_eq!(row.lifecycle, Lifecycle::Active);
    assert_eq!(row.generation, 0);
}

#[tokio::test]
async fn locked_secret_store_compensates_and_never_acknowledges() {
    let harness = fixture().await;
    harness.secrets.refuse_writes();
    let mut coordinator = harness.recovered().await;

    let error = coordinator
        .activate_credentials(OPERATION, &context(0), OperationKind::Setup, CANDIDATE)
        .await
        .unwrap_err();

    // The candidate write failed, so the orphan is compensated, the fence is
    // released, and the connection was not activated.
    assert_eq!(error, JournalError::Locked);
    assert!(harness.secrets.present().is_empty());
    assert_eq!(harness.secrets.deleted(), vec![CANDIDATE]);
    assert!(coordinator.journal().is_empty());
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
    assert_eq!(harness.repo.credential_ref(CONNECTION).await.unwrap(), None);
    let row = harness.repo.connection(CONNECTION).await.unwrap().unwrap();
    assert_eq!(row.lifecycle, Lifecycle::Active);
    assert_eq!(row.generation, 0);
}

#[tokio::test]
async fn failed_cleanup_after_commit_stays_pending_and_recovery_converges() {
    let harness = fixture().await;
    let mut coordinator = harness.recovered().await;
    setup(&mut coordinator, PREVIOUS).await;

    // Rotate while the superseded reference cannot be deleted.
    harness.secrets.refuse_delete(PREVIOUS);
    let error = coordinator
        .activate_credentials(
            "synthetic-operation-2",
            &context(1),
            OperationKind::ReplaceCredentials,
            CANDIDATE,
        )
        .await
        .unwrap_err();
    assert_eq!(error, JournalError::Locked);

    // The activation itself is committed and is not rolled back...
    assert_eq!(
        harness
            .repo
            .credential_ref(CONNECTION)
            .await
            .unwrap()
            .as_deref(),
        Some(CANDIDATE)
    );
    assert_eq!(
        harness
            .repo
            .connection(CONNECTION)
            .await
            .unwrap()
            .unwrap()
            .generation,
        2
    );
    // ...but the owed cleanup stays visible and the fence stays up.
    assert_eq!(harness.secrets.present(), vec![PREVIOUS, CANDIDATE]);
    let pending = coordinator.journal().get("synthetic-operation-2").unwrap();
    assert_eq!(pending.phase, OperationPhase::CompensationPending);
    assert_eq!(pending.previous_ref.as_deref(), Some(PREVIOUS));
    assert_eq!(
        coordinator.schedule_context(CONNECTION).await.unwrap_err(),
        JournalError::Conflict
    );
    assert_eq!(
        harness
            .repo
            .save_snapshot(&context(2), &snapshot(), at(1))
            .await
            .unwrap_err(),
        StorageError::Conflict
    );

    // A restart with the store still locked reports the same pending work twice:
    // recovery is idempotent and never declares success.
    drop(coordinator);
    let mut coordinator = harness.coordinator();
    let report = coordinator.recover().await.unwrap();
    assert!(report.finished.is_empty());
    assert_eq!(report.pending.len(), 2);
    assert_eq!(coordinator.recover().await.unwrap(), report);
    assert_eq!(harness.secrets.present(), vec![PREVIOUS, CANDIDATE]);

    // Unlocking lets the next recovery finish the owed cleanup and lift the fence.
    harness.secrets.allow_delete(PREVIOUS);
    let report = coordinator.recover().await.unwrap();
    assert!(report.pending.is_empty());
    assert_eq!(report.finished.len(), 2);
    assert_eq!(harness.secrets.present(), vec![CANDIDATE]);
    assert!(coordinator.journal().is_empty());
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
    assert_eq!(
        coordinator
            .schedule_context(CONNECTION)
            .await
            .unwrap()
            .generation,
        2
    );
    assert!(harness
        .repo
        .save_snapshot(&context(2), &snapshot(), at(1))
        .await
        .is_ok());
}

#[tokio::test]
async fn obsolete_generation_cannot_activate_and_the_candidate_is_compensated() {
    let harness = fixture().await;
    let mut coordinator = harness.recovered().await;
    // Disable behind the caller's back, so the captured context is obsolete.
    let stale = context(0);
    harness
        .repo
        .set_lifecycle(&stale, Lifecycle::Disabled)
        .await
        .unwrap();

    let error = coordinator
        .activate_credentials(
            OPERATION,
            &stale,
            OperationKind::ReplaceCredentials,
            CANDIDATE,
        )
        .await
        .unwrap_err();

    // The write transaction refused the stale generation and the orphan candidate
    // was compensated instead of being left behind.
    assert_eq!(error, JournalError::Conflict);
    assert_eq!(harness.repo.credential_ref(CONNECTION).await.unwrap(), None);
    assert_eq!(
        harness
            .repo
            .connection(CONNECTION)
            .await
            .unwrap()
            .unwrap()
            .generation,
        1
    );
    assert!(harness.secrets.present().is_empty());
    assert!(coordinator.journal().is_empty());
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn pending_projection_fences_result_writes_below_the_coordinator() {
    let harness = fixture().await;
    // An interrupted operation leaves an incomplete projection, which fences every
    // result write even when the caller bypasses the coordinator.
    let pending = record(
        OPERATION,
        OperationKind::Setup,
        OperationPhase::Pending,
        Some(CANDIDATE),
    );
    harness.repo.begin_operation(&pending).await.unwrap();

    assert_eq!(
        harness
            .repo
            .save_snapshot(&context(0), &snapshot(), at(1))
            .await
            .unwrap_err(),
        StorageError::Conflict
    );
    assert_eq!(
        harness.repo.operations(Some(CONNECTION)).await.unwrap(),
        vec![pending.clone()]
    );

    // Completing the projection lifts the fence for the same context.
    harness
        .repo
        .advance_operation(&OperationRecord {
            phase: OperationPhase::Complete,
            ..pending
        })
        .await
        .unwrap();
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
    assert!(harness
        .repo
        .save_snapshot(&context(0), &snapshot(), at(1))
        .await
        .is_ok());
}

#[tokio::test]
async fn deletion_ledger_fences_dispatch_and_rejects_a_restoring_manifest() {
    let harness = fixture().await;
    let mut coordinator = harness.recovered().await;
    setup(&mut coordinator, PREVIOUS).await;

    let generation = coordinator
        .begin_deletion("synthetic-operation-2", &context(1))
        .await
        .unwrap();

    // The ledger is durable before the transition, and dispatch is fenced.
    assert_eq!(generation, 2);
    assert_eq!(
        harness
            .repo
            .connection(CONNECTION)
            .await
            .unwrap()
            .unwrap()
            .lifecycle,
        Lifecycle::DeletionPending
    );
    assert!(coordinator.ledger().has_pending_cleanup());
    assert_eq!(
        coordinator.schedule_context(CONNECTION).await.unwrap_err(),
        JournalError::Conflict
    );
    assert_eq!(
        harness
            .repo
            .save_snapshot(&context(2), &snapshot(), at(1))
            .await
            .unwrap_err(),
        StorageError::Conflict
    );
    // Owed cleanup blocks every restore, and the secret is untouched.
    assert_eq!(
        coordinator.admit_restore(&manifest(&[])).unwrap_err(),
        JournalError::RecoveryRequired
    );
    assert_eq!(harness.secrets.present(), vec![PREVIOUS]);

    coordinator
        .finish_deletion("synthetic-operation-2")
        .await
        .unwrap();
    assert!(harness.secrets.present().is_empty());
    assert_eq!(harness.secrets.deleted(), vec![PREVIOUS]);
    assert!(!coordinator.ledger().has_pending_cleanup());
    assert!(coordinator.ledger().blocks(CONNECTION));
    assert!(coordinator.ledger().blocks(SCOPE));
    assert!(coordinator.journal().is_empty());
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
    assert!(coordinator.admit_restore(&manifest(&[])).is_ok());
    // A manifest still carrying a deleted identifier is rejected, so an
    // acknowledged deletion cannot return, and an invalid manifest is refused.
    assert_eq!(
        coordinator
            .admit_restore(&manifest(&[CONNECTION]))
            .unwrap_err(),
        JournalError::Conflict
    );
    assert_eq!(
        coordinator.admit_restore(&manifest(&[SCOPE])).unwrap_err(),
        JournalError::Conflict
    );
    assert_eq!(
        coordinator
            .admit_restore(&BackupManifest {
                backup_id: String::new(),
                ..manifest(&[])
            })
            .unwrap_err(),
        JournalError::Validation
    );
    assert!(coordinator
        .admit_restore(&manifest(&["synthetic-unrelated"]))
        .is_ok());

    // Model a restore of an earlier backup: the projection claims the connection
    // is active again and never heard of the deletion.
    harness
        .repo
        .execute_raw(
            "UPDATE connections SET lifecycle='active',generation=0,credential_ref=NULL WHERE id='synthetic-connection'",
        )
        .await
        .unwrap();
    harness
        .repo
        .execute_raw("DELETE FROM lifecycle_operations")
        .await
        .unwrap();
    let restored = harness.recovered().await;
    // The external ledger, not the database projection, keeps the fence.
    assert_eq!(
        restored.schedule_context(CONNECTION).await.unwrap_err(),
        JournalError::Conflict
    );
    assert_eq!(
        restored
            .admit_restore(&manifest(&[CONNECTION]))
            .unwrap_err(),
        JournalError::Conflict
    );
    assert!(restored
        .admit_restore(&manifest(&["synthetic-unrelated"]))
        .is_ok());
}

#[tokio::test]
async fn failed_deletion_cleanup_stays_visible_retryable_and_blocks_restore() {
    let harness = fixture().await;
    let mut coordinator = harness.recovered().await;
    setup(&mut coordinator, PREVIOUS).await;
    coordinator
        .begin_deletion("synthetic-operation-2", &context(1))
        .await
        .unwrap();
    harness.secrets.refuse_delete(PREVIOUS);

    // A failed cleanup is not an acknowledgement.
    assert_eq!(
        coordinator
            .finish_deletion("synthetic-operation-2")
            .await
            .unwrap_err(),
        JournalError::Locked
    );
    assert!(coordinator.ledger().has_pending_cleanup());
    assert_eq!(harness.secrets.present(), vec![PREVIOUS]);
    assert_eq!(
        coordinator
            .journal()
            .get("synthetic-operation-2")
            .unwrap()
            .phase,
        OperationPhase::Pending
    );

    // A restart cannot finish it either: recovery reports the pending work instead
    // of declaring success, and restore admission stays closed.
    drop(coordinator);
    let mut coordinator = harness.coordinator();
    let report = coordinator.recover().await.unwrap();
    assert!(report.finished.is_empty());
    assert!(report
        .pending
        .iter()
        .any(|action| matches!(action, RecoveryAction::DeleteReference { reference, .. } if reference == PREVIOUS)));
    assert_eq!(
        report
            .pending
            .iter()
            .filter(|action| matches!(action, RecoveryAction::DeletionPending { .. }))
            .count(),
        2
    );
    assert_eq!(
        coordinator.schedule_context(CONNECTION).await.unwrap_err(),
        JournalError::Conflict
    );
    assert_eq!(
        coordinator.admit_restore(&manifest(&[])).unwrap_err(),
        JournalError::RecoveryRequired
    );

    // The deletion is retryable and completes once the store is reachable.
    harness.secrets.allow_delete(PREVIOUS);
    let report = coordinator.recover().await.unwrap();
    assert!(report.pending.is_empty());
    assert!(harness.secrets.present().is_empty());
    assert!(!coordinator.ledger().has_pending_cleanup());
    assert!(coordinator.journal().is_empty());
    assert!(coordinator.admit_restore(&manifest(&[])).is_ok());
}

#[tokio::test]
async fn deletion_stops_before_any_secret_write_when_durability_fails() {
    let harness = fixture().await;
    let mut coordinator = harness.recovered().await;
    setup(&mut coordinator, PREVIOUS).await;
    harness.durable.fail_after(0);

    let error = coordinator
        .begin_deletion("synthetic-operation-2", &context(1))
        .await
        .unwrap_err();

    // The ledger write is the first step, so the lifecycle never transitioned and
    // no secret was touched.
    assert_eq!(error, JournalError::Durability);
    assert_eq!(
        harness
            .repo
            .connection(CONNECTION)
            .await
            .unwrap()
            .unwrap()
            .lifecycle,
        Lifecycle::Active
    );
    assert_eq!(harness.secrets.present(), vec![PREVIOUS]);
    assert!(harness.secrets.deleted().is_empty());
    assert!(harness.repo.operations(None).await.unwrap().is_empty());
    assert!(coordinator.ledger().is_empty());
    assert_eq!(
        coordinator
            .schedule_context(CONNECTION)
            .await
            .unwrap()
            .generation,
        1
    );
}

#[tokio::test]
async fn corrupt_external_state_blocks_recovery_instead_of_assuming_clean() {
    let harness = fixture().await;
    // Unparsable journal content is a visible recovery condition: absent readable
    // state must never be read as "cleanup already finished".
    harness.durable.put(JOURNAL_NAME, "not a journal");
    harness.open_err(JournalError::Corrupt);
    // A truncated record line fails the same way.
    harness.durable.put(
        JOURNAL_NAME,
        &format!("{}bogus\tline\n", Journal::default().encode()),
    );
    harness.open_err(JournalError::Corrupt);
    harness
        .durable
        .put(JOURNAL_NAME, &Journal::default().encode());
    harness.durable.put(LEDGER_NAME, "not a ledger");
    harness.open_err(JournalError::Corrupt);
    // No coordinator was returned, so nothing could be scheduled or mutated.
    assert_eq!(harness.durable.writes(), 0);
    assert!(harness.repo.operations(None).await.unwrap().is_empty());

    // Missing files are a fresh install, not corruption.
    let fresh = fixture().await;
    let mut coordinator = fresh.coordinator();
    assert_eq!(
        coordinator.recover().await.unwrap(),
        RecoveryReport::default()
    );
    assert_eq!(
        coordinator
            .schedule_context(CONNECTION)
            .await
            .unwrap()
            .generation,
        0
    );
}

#[tokio::test]
async fn recovery_removes_an_orphan_candidate_the_projection_never_recorded() {
    let harness = fixture().await;
    // An interruption after the journal write but before the projection commit:
    // the external journal owes a candidate the database never saw.
    let mut journal = Journal::default();
    journal
        .record(record(
            OPERATION,
            OperationKind::Setup,
            OperationPhase::CandidateWritten,
            Some(CANDIDATE),
        ))
        .unwrap();
    harness.durable.put(JOURNAL_NAME, &journal.encode());
    harness.secrets.seed(CANDIDATE);

    let mut coordinator = harness.coordinator();
    let report = coordinator.recover().await.unwrap();
    assert!(report.pending.is_empty());
    assert_eq!(report.finished.len(), 2);
    assert!(harness.secrets.present().is_empty());
    assert_eq!(harness.secrets.deleted(), vec![CANDIDATE]);
    assert!(coordinator.journal().is_empty());
    // The connection was never activated, so it keeps its committed state and
    // becomes schedulable again.
    assert_eq!(harness.repo.credential_ref(CONNECTION).await.unwrap(), None);
    assert_eq!(
        harness
            .repo
            .connection(CONNECTION)
            .await
            .unwrap()
            .unwrap()
            .lifecycle,
        Lifecycle::Active
    );
    assert_eq!(
        coordinator
            .schedule_context(CONNECTION)
            .await
            .unwrap()
            .generation,
        0
    );
    // A second run changes nothing.
    assert_eq!(
        coordinator.recover().await.unwrap(),
        RecoveryReport::default()
    );
}

#[tokio::test]
async fn one_locked_connection_does_not_stop_recovery_for_the_others() {
    let harness = fixture().await;
    active(&harness, OTHER, OTHER_SCOPE).await;
    let mut coordinator = harness.recovered().await;
    setup(&mut coordinator, PREVIOUS).await;
    coordinator
        .activate_credentials(
            "synthetic-operation-3",
            &other_context(0),
            OperationKind::Setup,
            "synthetic-reference-c",
        )
        .await
        .unwrap();
    // Both rotations leave their superseded reference owed.
    harness.secrets.refuse_delete(PREVIOUS);
    harness.secrets.refuse_delete("synthetic-reference-c");
    coordinator
        .activate_credentials(
            "synthetic-operation-2",
            &context(1),
            OperationKind::ReplaceCredentials,
            CANDIDATE,
        )
        .await
        .unwrap_err();
    coordinator
        .activate_credentials(
            "synthetic-operation-4",
            &other_context(1),
            OperationKind::ReplaceCredentials,
            "synthetic-reference-d",
        )
        .await
        .unwrap_err();
    drop(coordinator);

    // Only the other connection's store becomes reachable again.
    harness.secrets.allow_delete("synthetic-reference-c");
    let mut coordinator = harness.coordinator();
    let report = coordinator.recover().await.unwrap();
    assert!(report
        .pending
        .iter()
        .all(|action| action.connection_id() == Some(CONNECTION)));
    assert_eq!(report.pending.len(), 2);
    assert!(report
        .finished
        .iter()
        .all(|action| action.connection_id() == Some(OTHER)));
    assert_eq!(report.finished.len(), 2);

    // The locked connection stays fenced while the other one is schedulable.
    assert_eq!(
        coordinator.schedule_context(CONNECTION).await.unwrap_err(),
        JournalError::Conflict
    );
    assert_eq!(
        coordinator
            .schedule_context(OTHER)
            .await
            .unwrap()
            .generation,
        2
    );
}

#[tokio::test]
async fn recovery_runs_before_any_mutation_or_scheduling() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    // Nothing is admitted before recovery reconciles the external journals.
    assert_eq!(
        coordinator.schedule_context(CONNECTION).await.unwrap_err(),
        JournalError::RecoveryRequired
    );
    assert_eq!(
        coordinator.admit_restore(&manifest(&[])).unwrap_err(),
        JournalError::RecoveryRequired
    );
    assert_eq!(
        coordinator
            .activate_credentials(OPERATION, &context(0), OperationKind::Setup, CANDIDATE)
            .await
            .unwrap_err(),
        JournalError::RecoveryRequired
    );
    assert_eq!(
        coordinator
            .begin_deletion(OPERATION, &context(0))
            .await
            .unwrap_err(),
        JournalError::RecoveryRequired
    );
    assert_eq!(
        coordinator.finish_deletion(OPERATION).await.unwrap_err(),
        JournalError::RecoveryRequired
    );
    assert_eq!(harness.durable.writes(), 0);
    assert!(harness.secrets.present().is_empty());

    coordinator.recover().await.unwrap();
    assert_eq!(
        coordinator
            .schedule_context(CONNECTION)
            .await
            .unwrap()
            .generation,
        0
    );
}

#[tokio::test]
async fn external_state_carries_opaque_identifiers_only() {
    let harness = fixture().await;
    let mut coordinator = harness.recovered().await;
    setup(&mut coordinator, PREVIOUS).await;
    coordinator
        .activate_credentials(
            "synthetic-operation-2",
            &context(1),
            OperationKind::ReplaceCredentials,
            CANDIDATE,
        )
        .await
        .unwrap();
    coordinator
        .begin_deletion("synthetic-operation-3", &context(2))
        .await
        .unwrap();

    let journal = harness.durable.entry(JOURNAL_NAME).unwrap();
    let ledger = harness.durable.entry(LEDGER_NAME).unwrap();
    // Provider identity, the user-visible alias, the provider account key, usage
    // metrics, and every credential value the store holds must stay out.
    let forbidden = [
        value_for(PREVIOUS),
        value_for(CANDIDATE),
        "synthetic-provider".to_owned(),
        "Synthetic".to_owned(),
        format!("synthetic-account-{SCOPE}"),
        "synthetic-value".to_owned(),
    ];
    for text in [&journal, &ledger] {
        for needle in &forbidden {
            assert!(
                !text.contains(needle),
                "{needle} leaked into external state"
            );
        }
    }
    // Opaque identifiers and nonsecret state are present.
    assert!(journal.contains("synthetic-operation-3"));
    assert!(journal.contains(OperationKind::Delete.as_str()));
    assert!(journal.contains(OperationPhase::Pending.as_str()));
    assert!(ledger.contains(CONNECTION));
    assert!(ledger.contains(SCOPE));
}
