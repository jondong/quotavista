//! Synthetic fixtures for the secret stores. Every database, journal file, and
//! secret value here is throwaway; the native smoke checks are `#[ignore]`d,
//! use a disposable `quotavista-test-` service, and clean up after themselves.

use super::*;
use crate::{
    durable::FileStore,
    lifecycle::{Coordinator, JOURNAL_NAME, LEDGER_NAME},
    sqlite::SqliteRepository,
};
use quotavista_core::{
    domain::Lifecycle,
    journal::{DurableStore, JournalError, OperationKind, SecretStoreError},
    secrets::{mint_reference, validate_secret, SecretBackend, SessionSecrets},
    storage::{Connection, Repository, Scope},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tempfile::TempDir;

const CONNECTION: &str = "synthetic-connection";
const SCOPE: &str = "synthetic-scope";
const SECRET: &str = "synthetic-secret-value";
const ROTATED: &str = "synthetic-secret-value-rotated";

/// Failure-injectable store over the same port the native adapters implement.
/// Values stay in memory; failures simulate locked and unavailable backends.
#[derive(Debug, Default)]
struct FakeStore {
    state: Mutex<FakeState>,
}

#[derive(Debug, Default)]
struct FakeState {
    staged: BTreeMap<String, String>,
    present: BTreeMap<String, String>,
    deleted: Vec<String>,
    read_error: Option<SecretError>,
    refuse_writes: bool,
    refuse_deletes: BTreeSet<String>,
}

impl FakeStore {
    fn fail_read(&self, error: Option<SecretError>) {
        self.state.lock().unwrap().read_error = error;
    }

    fn refuse_writes(&self, refuse: bool) {
        self.state.lock().unwrap().refuse_writes = refuse;
    }

    fn refuse_delete(&self, reference: &str) {
        self.state
            .lock()
            .unwrap()
            .refuse_deletes
            .insert(reference.to_owned());
    }

    fn allow_delete(&self, reference: &str) {
        self.state.lock().unwrap().refuse_deletes.remove(reference);
    }
}

impl SecretStore for FakeStore {
    fn backend(&self) -> SecretBackend {
        SecretBackend::SessionOnly
    }

    fn stage_candidate(&self, reference: &str, secret: &str) -> Result<(), SecretError> {
        validate_secret(secret)?;
        self.state
            .lock()
            .unwrap()
            .staged
            .insert(reference.to_owned(), secret.to_owned());
        Ok(())
    }

    fn read(&self, reference: &str) -> Result<String, SecretError> {
        let state = self.state.lock().unwrap();
        if let Some(error) = state.read_error {
            return Err(error);
        }
        state
            .present
            .get(reference)
            .cloned()
            .ok_or(SecretError::NeedsCredentials)
    }
}

impl SecretReferenceStore for FakeStore {
    fn write_candidate(&self, reference: &str) -> Result<(), SecretStoreError> {
        let mut state = self.state.lock().unwrap();
        if state.refuse_writes {
            return Err(SecretStoreError::Locked);
        }
        match state.staged.remove(reference) {
            Some(value) => {
                state.present.insert(reference.to_owned(), value);
                Ok(())
            }
            None => Err(SecretStoreError::Unavailable),
        }
    }

    fn delete_reference(&self, reference: &str) -> Result<(), SecretStoreError> {
        let mut state = self.state.lock().unwrap();
        if state.refuse_deletes.contains(reference) {
            return Err(SecretStoreError::Locked);
        }
        state.staged.remove(reference);
        state.present.remove(reference);
        state.deleted.push(reference.to_owned());
        Ok(())
    }
}

struct Harness {
    dir: TempDir,
    repo: SqliteRepository,
}

async fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let repo = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    repo.create_connection(&connection(Lifecycle::Active), &scope())
        .await
        .unwrap();
    Harness { dir, repo }
}

fn connection(lifecycle: Lifecycle) -> Connection {
    Connection {
        id: CONNECTION.into(),
        provider: format!("synthetic-provider-{CONNECTION}"),
        alias: format!("Synthetic {CONNECTION}"),
        generation: 0,
        lifecycle,
    }
}

fn scope() -> Scope {
    Scope {
        id: SCOPE.into(),
        connection_id: CONNECTION.into(),
        key: format!("synthetic-account-{SCOPE}"),
        kind: "account".into(),
    }
}

fn durable(harness: &Harness) -> FileStore {
    FileStore::new(harness.dir.path().join("safety")).unwrap()
}

fn reference(name: &str) -> String {
    mint_reference(SecretBackend::SessionOnly, name)
}

/// Setup, rotation, and deletion all flow through the journal-backed
/// lifecycle with the session-only store behind the same port.
#[tokio::test]
async fn session_only_credentials_flow_through_the_journal_backed_lifecycle() {
    let harness = harness().await;
    let store = Arc::new(SessionSecrets::new());
    let mut coordinator =
        Coordinator::open(harness.repo.clone(), durable(&harness), store.clone()).unwrap();
    coordinator.recover().await.unwrap();

    // Setup: the staged candidate commits at the journaled boundary.
    let first = reference("synthetic-candidate-a");
    store.stage_candidate(&first, SECRET).unwrap();
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .activate_credentials(
            "synthetic-operation-a",
            &context,
            OperationKind::Setup,
            &first,
        )
        .await
        .unwrap();
    assert_eq!(store.read(&first).unwrap(), SECRET);

    // Rotation: the new candidate activates, the superseded one is cleaned up.
    let second = reference("synthetic-candidate-b");
    store.stage_candidate(&second, ROTATED).unwrap();
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .activate_credentials(
            "synthetic-operation-b",
            &context,
            OperationKind::ReplaceCredentials,
            &second,
        )
        .await
        .unwrap();
    assert_eq!(store.read(&second).unwrap(), ROTATED);
    assert_eq!(store.read(&first), Err(SecretError::NeedsCredentials));

    // Deletion removes the active secret.
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .begin_deletion("synthetic-operation-c", &context)
        .await
        .unwrap();
    coordinator
        .finish_deletion("synthetic-operation-c")
        .await
        .unwrap();
    assert_eq!(store.read(&second), Err(SecretError::NeedsCredentials));
}

/// A failed candidate write keeps the previous credential active, and a
/// cleanup that cannot finish stays visibly pending for recovery (A18).
#[tokio::test]
async fn failed_rotation_keeps_previous_and_failed_compensation_stays_visible() {
    let harness = harness().await;
    let store = Arc::new(FakeStore::default());
    let mut coordinator =
        Coordinator::open(harness.repo.clone(), durable(&harness), store.clone()).unwrap();
    coordinator.recover().await.unwrap();

    let first = reference("synthetic-candidate-a");
    store.stage_candidate(&first, SECRET).unwrap();
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .activate_credentials(
            "synthetic-operation-a",
            &context,
            OperationKind::Setup,
            &first,
        )
        .await
        .unwrap();

    // The candidate write fails with a locked store: the previous credential
    // stays active and retrievable, and the fence is released after the abort.
    let second = reference("synthetic-candidate-b");
    store.stage_candidate(&second, ROTATED).unwrap();
    store.refuse_writes(true);
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    let error = coordinator
        .activate_credentials(
            "synthetic-operation-b",
            &context,
            OperationKind::ReplaceCredentials,
            &second,
        )
        .await
        .unwrap_err();
    assert_eq!(error, JournalError::Locked);
    assert_eq!(store.read(&first).unwrap(), SECRET);
    assert_eq!(
        harness.repo.credential_ref(CONNECTION).await.unwrap(),
        Some(first.clone())
    );
    store.refuse_writes(false);
    assert!(coordinator.schedule_context(CONNECTION).await.is_ok());

    // The next rotation commits, but removing the superseded reference fails:
    // the operation stays visibly pending compensation instead of
    // acknowledging, and the previous secret is preserved meanwhile.
    let third = reference("synthetic-candidate-c");
    store.stage_candidate(&third, ROTATED).unwrap();
    store.refuse_delete(&first);
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    let error = coordinator
        .activate_credentials(
            "synthetic-operation-c",
            &context,
            OperationKind::ReplaceCredentials,
            &third,
        )
        .await
        .unwrap_err();
    assert_eq!(error, JournalError::Locked);
    assert_eq!(store.read(&third).unwrap(), ROTATED);
    assert_eq!(store.read(&first).unwrap(), SECRET);
    assert_eq!(
        harness.repo.credential_ref(CONNECTION).await.unwrap(),
        Some(third.clone())
    );

    // A restart finds the owed cleanup in the journal and keeps it pending
    // while the store still refuses; once unlocked, recovery finishes it.
    drop(coordinator);
    let mut restarted =
        Coordinator::open(harness.repo.clone(), durable(&harness), store.clone()).unwrap();
    let report = restarted.recover().await.unwrap();
    assert!(!report.pending.is_empty());
    assert_eq!(store.read(&first).unwrap(), SECRET);
    store.allow_delete(&first);
    let report = restarted.recover().await.unwrap();
    assert!(report.pending.is_empty());
    assert_eq!(store.read(&first), Err(SecretError::NeedsCredentials));
}

/// Session-only is explicit opt-in: it works until exit, then the restart
/// retains the nonsecret data, finds no retrievable secret, and the
/// connection enters needs-credentials (A21).
#[tokio::test]
async fn session_only_restart_loses_the_secret_and_enters_needs_credentials() {
    let harness = harness().await;
    let store = Arc::new(SessionSecrets::new());
    let mut coordinator =
        Coordinator::open(harness.repo.clone(), durable(&harness), store.clone()).unwrap();
    coordinator.recover().await.unwrap();

    let candidate = reference("synthetic-candidate-a");
    store.stage_candidate(&candidate, SECRET).unwrap();
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .activate_credentials(
            "synthetic-operation-a",
            &context,
            OperationKind::Setup,
            &candidate,
        )
        .await
        .unwrap();
    assert_eq!(store.read(&candidate).unwrap(), SECRET);

    // Restart: journals and the database survive, process memory does not.
    drop(coordinator);
    let restarted = Arc::new(SessionSecrets::new());
    let mut coordinator =
        Coordinator::open(harness.repo.clone(), durable(&harness), restarted.clone()).unwrap();
    let report = coordinator.recover().await.unwrap();
    assert!(report.pending.is_empty());

    // Nonsecret configuration is retained; the session secret is gone.
    let connection = harness.repo.connection(CONNECTION).await.unwrap().unwrap();
    assert_eq!(connection.alias, format!("Synthetic {CONNECTION}"));
    assert_eq!(connection.lifecycle, Lifecycle::Active);
    assert_eq!(
        restarted.read(&candidate),
        Err(SecretError::NeedsCredentials)
    );

    // Reconciliation enters needs-credentials instead of falling back.
    assert!(
        !reconcile_connection_credentials(&harness.repo, restarted.as_ref(), CONNECTION)
            .await
            .unwrap()
    );
    let connection = harness.repo.connection(CONNECTION).await.unwrap().unwrap();
    assert_eq!(connection.lifecycle, Lifecycle::NeedsCredentials);
}

/// Locked and unavailable stay distinct end to end, and neither moves the
/// connection: no downgrade, no plaintext fallback (A21).
#[tokio::test]
async fn locked_and_unavailable_are_distinct_and_leave_the_connection_untouched() {
    let harness = harness().await;
    let store = Arc::new(SessionSecrets::new());
    let mut coordinator =
        Coordinator::open(harness.repo.clone(), durable(&harness), store.clone()).unwrap();
    coordinator.recover().await.unwrap();
    let candidate = reference("synthetic-candidate-a");
    store.stage_candidate(&candidate, SECRET).unwrap();
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .activate_credentials(
            "synthetic-operation-a",
            &context,
            OperationKind::Setup,
            &candidate,
        )
        .await
        .unwrap();

    let failing = FakeStore::default();
    failing.fail_read(Some(SecretError::Locked));
    assert_eq!(
        reconcile_connection_credentials(&harness.repo, &failing, CONNECTION).await,
        Err(SecretError::Locked)
    );
    assert_eq!(lifecycle_of(&harness).await, Lifecycle::Active);

    failing.fail_read(Some(SecretError::Unavailable));
    assert_eq!(
        reconcile_connection_credentials(&harness.repo, &failing, CONNECTION).await,
        Err(SecretError::Unavailable)
    );
    assert_eq!(lifecycle_of(&harness).await, Lifecycle::Active);
}

async fn lifecycle_of(harness: &Harness) -> Lifecycle {
    harness
        .repo
        .connection(CONNECTION)
        .await
        .unwrap()
        .unwrap()
        .lifecycle
}

/// No synthetic secret value ever reaches the external journals or the SQLite
/// database; only opaque references do (A10).
#[tokio::test]
async fn secret_values_never_reach_journals_or_the_database() {
    let harness = harness().await;
    let store = Arc::new(SessionSecrets::new());
    let mut coordinator =
        Coordinator::open(harness.repo.clone(), durable(&harness), store.clone()).unwrap();
    coordinator.recover().await.unwrap();

    let first = reference("synthetic-candidate-a");
    store.stage_candidate(&first, SECRET).unwrap();
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .activate_credentials(
            "synthetic-operation-a",
            &context,
            OperationKind::Setup,
            &first,
        )
        .await
        .unwrap();
    let second = reference("synthetic-candidate-b");
    store.stage_candidate(&second, ROTATED).unwrap();
    let context = coordinator.schedule_context(CONNECTION).await.unwrap();
    coordinator
        .activate_credentials(
            "synthetic-operation-b",
            &context,
            OperationKind::ReplaceCredentials,
            &second,
        )
        .await
        .unwrap();

    let files = durable(&harness);
    for name in [JOURNAL_NAME, LEDGER_NAME] {
        let contents = files.read(name).unwrap().unwrap_or_default();
        assert!(!contents.contains(SECRET), "{name} holds a secret value");
        assert!(!contents.contains(ROTATED), "{name} holds a secret value");
    }
    // Sanity: the journal files exist and were read, so the check is real.
    let journal = files.read(JOURNAL_NAME).unwrap();
    assert!(journal.is_some());

    // WAL mode keeps uncommitted pages in the sidecar files; check them all.
    let mut database = String::new();
    for suffix in [
        "synthetic.sqlite",
        "synthetic.sqlite-wal",
        "synthetic.sqlite-shm",
    ] {
        let path = harness.dir.path().join(suffix);
        if path.exists() {
            database.push_str(&String::from_utf8_lossy(&std::fs::read(path).unwrap()));
        }
    }
    assert!(!database.contains(SECRET));
    assert!(!database.contains(ROTATED));
    // Sanity: nonsecret configuration and the opaque reference do live in the
    // database, so the check is real.
    assert!(database.contains("synthetic-provider-"));
    assert!(database.contains("qv1:session-only:synthetic-candidate-b"));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn native_store_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<NativeSecrets>();
}

/// Shared smoke-check body: synthetic create/read/rotate/delete against one
/// native store. Never asserts before cleanup, so no entry is left behind.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn native_roundtrip(store: &NativeSecrets, backend: SecretBackend, service: &str) {
    let first = mint_reference(backend, "synthetic-entry-a");
    let second = mint_reference(backend, "synthetic-entry-b");
    let mut ok = store
        .stage_candidate(&first, "synthetic-secret-value-a")
        .is_ok();
    ok &= store.write_candidate(&first).is_ok();
    ok &= store.read(&first).as_deref() == Ok("synthetic-secret-value-a");
    // Rotation: the previous entry stays until the candidate commits.
    ok &= store
        .stage_candidate(&second, "synthetic-secret-value-b")
        .is_ok();
    ok &= store.write_candidate(&second).is_ok();
    ok &= store.read(&second).as_deref() == Ok("synthetic-secret-value-b");
    ok &= store.read(&first).as_deref() == Ok("synthetic-secret-value-a");
    // Cleanup always runs, and deletion of an absent entry succeeds.
    ok &= store.delete_reference(&first).is_ok();
    ok &= store.delete_reference(&second).is_ok();
    ok &= store.delete_reference(&second).is_ok();
    let cleaned = store.read(&first) == Err(SecretError::NeedsCredentials)
        && store.read(&second) == Err(SecretError::NeedsCredentials);
    println!(
        "cleanup evidence: {first} and {second} removed from service {service}; \
         both read back as NeedsCredentials"
    );
    assert!(ok && cleaned, "native smoke check failed; see output above");
}

/// macOS Keychain smoke check. Disposable synthetic secrets only, under a
/// uniquely named `quotavista-test-` service. Run manually with
/// `cargo test -p quotavista-infrastructure -- --ignored`.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "native smoke check: touches the host keychain with synthetic entries"]
fn macos_keychain_synthetic_roundtrip() {
    let service = format!("quotavista-test-p1-05-{}", std::process::id());
    println!("host: {}", std::env::var("HOSTNAME").unwrap_or_default());
    println!("service: {service}");
    let store = NativeSecrets::open_with_service(&service)
        .expect("macOS Keychain unavailable on this host; record the check as pending");
    native_roundtrip(&store, SecretBackend::MacosKeychain, &service);
}

/// Linux Secret Service smoke check. A headless host has no session bus: the
/// check records itself as pending instead of failing.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "native smoke check: touches the host secret service with synthetic entries"]
fn linux_secret_service_synthetic_roundtrip() {
    let service = format!("quotavista-test-p1-05-{}", std::process::id());
    println!("host: {}", std::env::var("HOSTNAME").unwrap_or_default());
    println!("service: {service}");
    let store = match NativeSecrets::open_with_service(&service) {
        Ok(store) => store,
        Err(error) => {
            println!(
                "pending: Linux Secret Service unavailable on this host ({error:?}); \
                 no session bus"
            );
            return;
        }
    };
    native_roundtrip(&store, SecretBackend::LinuxSecretService, &service);
}
