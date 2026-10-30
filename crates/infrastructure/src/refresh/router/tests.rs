//! Offline router tests: fake transports record every call, so any network
//! traffic from a path that must stay local fails loudly. The repository is a
//! throwaway SQLite file, the clock is manual, and credentials live in the
//! in-memory session store.

use super::*;
use crate::refresh::{Admission, HistoryOutcome, Outcome, RefreshCoordinator};
use crate::sqlite::SqliteRepository;
use chrono::{DateTime, Utc};
use quotavista_core::{
    domain::{Lifecycle, Provenance, QueryHealth},
    journal::{DurableStore, JournalError, OperationKind, COOLDOWNS_NAME},
    provider::RefreshClock,
    secrets::{mint_reference, SecretBackend, SecretStore, SessionSecrets},
    storage::{Connection, Repository, Scope, WriteContext},
};
use quotavista_providers::{
    openrouter::{TransportError, TransportResponse},
    simulated::{SimulatedAdapter, SimulatedRouter},
    synthetic_providers,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

type Router = ProviderRouter<SqliteRepository, SessionSecrets, FakeTransport, FakeTransport>;
type RouterCoordinator = RefreshCoordinator<SqliteRepository, Router, FakeDurable, ManualClock>;

const KIMI_OK: &str =
    r#"{"data":{"shared_pool":{"consumed_percent":"30","refreshes_at":"2026-11-01T00:00:00Z"}}}"#;
const OPENROUTER_OK: &str = r#"{"data":{"total_credits":"120.50","total_usage":"37.25"}}"#;

fn ok_response(body: &str) -> Result<TransportResponse, TransportError> {
    Ok(TransportResponse {
        status: 200,
        retry_after: None,
        body: body.as_bytes().to_vec(),
    })
}

/// Records every request path and answers from a scripted queue; an exhausted
/// queue reports `Offline` so unexpected calls fail the test loudly.
#[derive(Clone, Default)]
struct FakeTransport {
    calls: Arc<Mutex<Vec<String>>>,
    responses: Arc<Mutex<VecDeque<Result<TransportResponse, TransportError>>>>,
}

impl FakeTransport {
    fn script(&self, response: Result<TransportResponse, TransportError>) {
        self.responses.lock().unwrap().push_back(response);
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn answer(&self, path: &str) -> Result<TransportResponse, TransportError> {
        self.calls.lock().unwrap().push(path.to_owned());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(TransportError::Offline))
    }
}

impl OpenRouterTransport for FakeTransport {
    async fn get(
        &self,
        _token: &BearerToken,
        path: &str,
    ) -> Result<TransportResponse, TransportError> {
        self.answer(path)
    }
}

impl KimiTransport for FakeTransport {
    async fn get(
        &self,
        _token: &BearerToken,
        path: &str,
    ) -> Result<TransportResponse, TransportError> {
        self.answer(path)
    }
}

#[derive(Clone, Debug)]
struct ManualClock {
    state: Arc<Mutex<DateTime<Utc>>>,
}

impl ManualClock {
    fn at(now: DateTime<Utc>) -> Self {
        Self {
            state: Arc::new(Mutex::new(now)),
        }
    }
}

impl RefreshClock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.state.lock().unwrap()
    }
}

#[derive(Clone, Debug, Default)]
struct FakeDurable {
    entries: Arc<Mutex<BTreeMap<String, String>>>,
}

impl FakeDurable {
    fn entry(&self, name: &str) -> Option<String> {
        self.entries.lock().unwrap().get(name).cloned()
    }
}

impl DurableStore for FakeDurable {
    fn read(&self, name: &str) -> Result<Option<String>, JournalError> {
        Ok(self.entries.lock().unwrap().get(name).cloned())
    }

    fn replace(&self, name: &str, contents: &str) -> Result<(), JournalError> {
        self.entries
            .lock()
            .unwrap()
            .insert(name.into(), contents.into());
        Ok(())
    }
}

fn at(hour: u32) -> DateTime<Utc> {
    format!("2026-01-01T{hour:02}:00:00Z").parse().unwrap()
}

fn connection(id: &str, provider: &str, lifecycle: Lifecycle) -> Connection {
    Connection {
        id: id.into(),
        provider: provider.into(),
        alias: format!("Alias {id}"),
        generation: 0,
        lifecycle,
    }
}

fn scope(connection_id: &str) -> Scope {
    Scope {
        id: format!("scope-{connection_id}"),
        connection_id: connection_id.into(),
        key: format!("account-{connection_id}"),
        kind: "account".into(),
    }
}

struct Harness {
    _dir: TempDir,
    repo: SqliteRepository,
    durable: FakeDurable,
    clock: ManualClock,
    secrets: Arc<SessionSecrets>,
    openrouter: FakeTransport,
    kimi: FakeTransport,
}

impl Harness {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = SqliteRepository::open(&dir.path().join("router.sqlite"))
            .await
            .unwrap();
        Self {
            _dir: dir,
            repo,
            durable: FakeDurable::default(),
            clock: ManualClock::at(at(12)),
            secrets: Arc::new(SessionSecrets::new()),
            openrouter: FakeTransport::default(),
            kimi: FakeTransport::default(),
        }
    }

    fn coordinator(&self, simulated: SimulatedRouter) -> RouterCoordinator {
        self.coordinator_with(
            Some(self.secrets.clone()),
            Some(self.openrouter.clone()),
            Some(self.kimi.clone()),
            simulated,
        )
    }

    fn coordinator_with(
        &self,
        secrets: Option<Arc<SessionSecrets>>,
        openrouter: Option<FakeTransport>,
        kimi: Option<FakeTransport>,
        simulated: SimulatedRouter,
    ) -> RouterCoordinator {
        let router = ProviderRouter::new(self.repo.clone(), secrets, openrouter, kimi, simulated);
        RefreshCoordinator::open(
            self.repo.clone(),
            router,
            self.durable.clone(),
            self.clock.clone(),
        )
        .unwrap()
    }

    /// An Active connection with no credential reference.
    async fn add_connection(&self, id: &str, provider: &str) {
        self.repo
            .create_connection(&connection(id, provider, Lifecycle::Active), &scope(id))
            .await
            .unwrap();
    }

    /// A live connection provisioned through the real lifecycle coordinator:
    /// staged session credential, journaled setup, activation. Returns the
    /// generation after activation.
    async fn add_live(&self, id: &str, provider: &str, secret: &str) -> i64 {
        self.repo
            .create_connection(
                &connection(id, provider, Lifecycle::PendingSetup),
                &scope(id),
            )
            .await
            .unwrap();
        let reference = mint_reference(SecretBackend::SessionOnly, &format!("ref-{id}"));
        self.secrets.stage_candidate(&reference, secret).unwrap();
        let mut lifecycle = crate::lifecycle::Coordinator::open(
            self.repo.clone(),
            self.durable.clone(),
            self.secrets.clone(),
        )
        .unwrap();
        lifecycle.recover().await.unwrap();
        lifecycle
            .activate_credentials(
                &format!("op-{id}"),
                &WriteContext {
                    connection_id: id.into(),
                    generation: 0,
                },
                OperationKind::Setup,
                &reference,
            )
            .await
            .unwrap()
    }

    /// Write context matching the connection's current generation.
    async fn context_for(&self, id: &str) -> WriteContext {
        let row = self.repo.connection(id).await.unwrap().unwrap();
        WriteContext {
            connection_id: id.into(),
            generation: row.generation,
        }
    }
}

async fn refresh(harness: &Harness, coordinator: &mut RouterCoordinator, id: &str) -> Outcome {
    let context = harness.context_for(id).await;
    let admissions = coordinator.request_manual(&[(id, context.clone())]).await;
    let Admission::Admitted { job } = admissions[0].clone() else {
        panic!("request for {id} must be admitted: {admissions:?}");
    };
    run_to_completion(coordinator, &job).await
}

async fn refresh_pair(
    harness: &Harness,
    coordinator: &mut RouterCoordinator,
    first: &str,
    second: &str,
) -> (Outcome, Outcome) {
    let contexts = [
        (first, harness.context_for(first).await),
        (second, harness.context_for(second).await),
    ];
    let requests: Vec<(&str, WriteContext)> = contexts
        .iter()
        .map(|(id, context)| (*id, context.clone()))
        .collect();
    let admissions = coordinator.request_manual(&requests).await;
    let mut outcomes = Vec::new();
    for admission in admissions {
        let Admission::Admitted { job } = admission else {
            panic!("both requests must be admitted");
        };
        outcomes.push(run_to_completion(coordinator, &job).await);
    }
    (outcomes.remove(0), outcomes.remove(0))
}

async fn run_to_completion(coordinator: &mut RouterCoordinator, job: &str) -> Outcome {
    loop {
        if let Some(outcome) = coordinator.outcome(job) {
            return outcome;
        }
        coordinator.pump().await;
    }
}

#[tokio::test]
async fn demo_connection_uses_simulated_adapter_and_never_calls_transports() {
    let harness = Harness::new().await;
    harness.add_connection("demo-kimi", "synthetic-kimi").await;
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    let outcome = refresh(&harness, &mut coordinator, "demo-kimi").await;

    assert!(matches!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Published { .. }
        }
    ));
    assert_eq!(harness.kimi.call_count(), 0);
    assert_eq!(harness.openrouter.call_count(), 0);
    let snapshot = harness
        .repo
        .snapshot("scope-demo-kimi", None, "tokens")
        .await
        .unwrap()
        .expect("demo refresh commits current values");
    assert_eq!(snapshot.snapshot.provenance, Provenance::Synthetic);
}

#[tokio::test]
async fn live_kimi_connection_resolves_credential_and_commits_observation() {
    let harness = Harness::new().await;
    harness.add_live("user-kimi", "kimi", "sk-test-kimi").await;
    harness.kimi.script(ok_response(KIMI_OK));
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    let outcome = refresh(&harness, &mut coordinator, "user-kimi").await;

    // The live adapters serve no history yet, so the job completes skipped.
    assert_eq!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Skipped
        }
    );
    assert_eq!(harness.kimi.call_count(), 1);
    assert_eq!(harness.openrouter.call_count(), 0);
    let snapshot = harness
        .repo
        .snapshot("scope-user-kimi", None, "quota_consumed")
        .await
        .unwrap()
        .expect("live refresh commits membership metrics");
    assert_eq!(snapshot.snapshot.provenance, Provenance::Observation);
    assert_eq!(
        coordinator
            .health()
            .get(&("user-kimi".into(), "shared-pool".into())),
        Some(&QueryHealth::Success)
    );
}

#[tokio::test]
async fn live_openrouter_connection_uses_only_the_openrouter_transport() {
    let harness = Harness::new().await;
    harness
        .add_live("user-openrouter", "openrouter", "sk-test-openrouter")
        .await;
    harness.openrouter.script(ok_response(OPENROUTER_OK));
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    let outcome = refresh(&harness, &mut coordinator, "user-openrouter").await;

    assert_eq!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Skipped
        }
    );
    assert_eq!(harness.openrouter.call_count(), 1);
    assert_eq!(harness.kimi.call_count(), 0);
    let snapshot = harness
        .repo
        .snapshot("scope-user-openrouter", None, "remaining")
        .await
        .unwrap()
        .expect("live refresh commits credits metrics");
    assert_eq!(snapshot.snapshot.provenance, Provenance::Observation);
}

#[tokio::test]
async fn missing_credential_fails_permission_required_without_network() {
    let harness = Harness::new().await;
    harness.add_connection("user-kimi", "kimi").await;
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    let outcome = refresh(&harness, &mut coordinator, "user-kimi").await;

    assert_eq!(
        outcome,
        Outcome::Failed {
            error: ProviderError::PermissionRequired
        }
    );
    assert_eq!(harness.kimi.call_count(), 0);
    assert_eq!(harness.openrouter.call_count(), 0);
    assert_eq!(
        coordinator
            .health()
            .get(&("user-kimi".into(), String::new())),
        Some(&QueryHealth::PermissionRequired)
    );
}

#[tokio::test]
async fn unknown_provider_fails_typed_without_network() {
    let harness = Harness::new().await;
    harness.add_connection("mystery", "mystery-vendor").await;
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    let outcome = refresh(&harness, &mut coordinator, "mystery").await;

    assert_eq!(
        outcome,
        Outcome::Failed {
            error: ProviderError::InvalidResponse
        }
    );
    assert_eq!(harness.kimi.call_count(), 0);
    assert_eq!(harness.openrouter.call_count(), 0);
}

#[tokio::test]
async fn missing_transport_marks_live_connection_offline_without_network() {
    let harness = Harness::new().await;
    harness.add_connection("user-kimi", "kimi").await;
    let mut coordinator = harness.coordinator_with(
        Some(harness.secrets.clone()),
        Some(harness.openrouter.clone()),
        None,
        SimulatedRouter::new(),
    );

    let outcome = refresh(&harness, &mut coordinator, "user-kimi").await;

    assert_eq!(
        outcome,
        Outcome::Failed {
            error: ProviderError::Offline
        }
    );
    assert_eq!(harness.kimi.call_count(), 0);
}

#[tokio::test]
async fn degraded_composition_without_secrets_keeps_demo_connections_working() {
    let harness = Harness::new().await;
    harness.add_connection("user-kimi", "kimi").await;
    harness.add_connection("demo-kimi", "synthetic-kimi").await;
    let mut coordinator = harness.coordinator_with(
        None,
        Some(harness.openrouter.clone()),
        Some(harness.kimi.clone()),
        SimulatedRouter::new(),
    );

    let live = refresh(&harness, &mut coordinator, "user-kimi").await;
    let demo = refresh(&harness, &mut coordinator, "demo-kimi").await;

    assert_eq!(
        live,
        Outcome::Failed {
            error: ProviderError::PermissionRequired
        }
    );
    assert!(matches!(demo, Outcome::Completed { .. }));
    assert_eq!(harness.kimi.call_count(), 0);
    assert_eq!(harness.openrouter.call_count(), 0);
}

#[tokio::test]
async fn dataset_failure_is_isolated_and_prior_snapshots_are_retained() {
    let harness = Harness::new().await;
    harness
        .add_live("user-openrouter", "openrouter", "sk-test-openrouter")
        .await;
    harness.add_live("user-kimi", "kimi", "sk-test-kimi").await;
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    // First round: both live connections commit.
    harness.openrouter.script(ok_response(OPENROUTER_OK));
    harness.kimi.script(ok_response(KIMI_OK));
    let (first_openrouter, first_kimi) =
        refresh_pair(&harness, &mut coordinator, "user-openrouter", "user-kimi").await;
    assert!(matches!(first_openrouter, Outcome::Completed { .. }));
    assert!(matches!(first_kimi, Outcome::Completed { .. }));
    let committed = harness
        .repo
        .snapshot("scope-user-openrouter", None, "remaining")
        .await
        .unwrap()
        .expect("first refresh commits credits");

    // Second round: OpenRouter answers 401 for the credits dataset while Kimi
    // stays healthy. The dataset failure must not touch the committed values
    // and must not affect the other connection.
    harness.openrouter.script(Ok(TransportResponse {
        status: 401,
        retry_after: None,
        body: br#"{"error":"unauthorized"}"#.to_vec(),
    }));
    harness.kimi.script(ok_response(KIMI_OK));
    let (second_openrouter, second_kimi) =
        refresh_pair(&harness, &mut coordinator, "user-openrouter", "user-kimi").await;

    // A dataset-level failure still completes the job; only the dataset's
    // health records the permission problem.
    assert!(matches!(second_openrouter, Outcome::Completed { .. }));
    assert!(matches!(second_kimi, Outcome::Completed { .. }));
    assert_eq!(
        coordinator
            .health()
            .get(&("user-openrouter".into(), "prepaid-credits".into())),
        Some(&QueryHealth::PermissionRequired)
    );
    assert_eq!(
        coordinator
            .health()
            .get(&("user-kimi".into(), "shared-pool".into())),
        Some(&QueryHealth::Success)
    );
    let retained = harness
        .repo
        .snapshot("scope-user-openrouter", None, "remaining")
        .await
        .unwrap()
        .expect("failed dataset retains its last committed value");
    assert_eq!(retained.snapshot.value, committed.snapshot.value);
    assert_eq!(
        retained.snapshot.observed_at,
        committed.snapshot.observed_at
    );
}

#[tokio::test]
async fn whole_call_failure_retains_prior_snapshots() {
    let harness = Harness::new().await;
    harness.add_live("user-kimi", "kimi", "sk-test-kimi").await;
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    harness.kimi.script(ok_response(KIMI_OK));
    let first = refresh(&harness, &mut coordinator, "user-kimi").await;
    assert!(matches!(first, Outcome::Completed { .. }));
    let committed = harness
        .repo
        .snapshot("scope-user-kimi", None, "quota_consumed")
        .await
        .unwrap()
        .unwrap();

    harness.kimi.script(Err(TransportError::Offline));
    let second = refresh(&harness, &mut coordinator, "user-kimi").await;

    assert_eq!(
        second,
        Outcome::Failed {
            error: ProviderError::Offline
        }
    );
    let retained = harness
        .repo
        .snapshot("scope-user-kimi", None, "quota_consumed")
        .await
        .unwrap()
        .expect("last committed values survive a whole-call failure");
    assert_eq!(retained.snapshot.value, committed.snapshot.value);
    assert_eq!(
        retained.snapshot.observed_at,
        committed.snapshot.observed_at
    );
}

#[tokio::test]
async fn rate_limit_persists_cooldown_and_blocks_readmission_without_network() {
    let harness = Harness::new().await;
    harness.add_live("user-kimi", "kimi", "sk-test-kimi").await;
    harness.kimi.script(Ok(TransportResponse {
        status: 429,
        retry_after: Some("120".into()),
        body: br#"{"error":"slow down"}"#.to_vec(),
    }));
    let mut coordinator = harness.coordinator(SimulatedRouter::new());

    let outcome = refresh(&harness, &mut coordinator, "user-kimi").await;

    assert_eq!(
        outcome,
        Outcome::Cooldown {
            until: at(12) + chrono::Duration::seconds(120)
        }
    );
    assert!(
        harness.durable.entry(COOLDOWNS_NAME).is_some(),
        "cooldown persists across restart"
    );
    // A blocked re-request is refused before any provider call.
    let context = harness.context_for("user-kimi").await;
    let admissions = coordinator.request_manual(&[("user-kimi", context)]).await;
    assert!(matches!(admissions[0], Admission::CooldownBlocked { .. }));
    assert_eq!(harness.kimi.call_count(), 1);
}

#[tokio::test]
async fn single_adapter_override_answers_live_connections_without_network() {
    let harness = Harness::new().await;
    harness.add_live("user-kimi", "kimi", "sk-test-kimi").await;
    let mut coordinator = harness.coordinator(SimulatedRouter::single(SimulatedAdapter::new(
        synthetic_providers()[0],
    )));

    let outcome = refresh(&harness, &mut coordinator, "user-kimi").await;

    assert!(matches!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Published { .. }
        }
    ));
    assert_eq!(harness.kimi.call_count(), 0);
    assert_eq!(harness.openrouter.call_count(), 0);
}
