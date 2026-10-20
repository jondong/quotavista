//! Deterministic harness for the refresh coordinator. The SQLite file lives in
//! a throwaway temp directory, the clock is manual, and the provider is the
//! synthetic adapter — no real provider account or network is involved.
use super::*;
use crate::sqlite::SqliteRepository;
use chrono::{DateTime, Utc};
use quotavista_core::{
    domain::{Availability, Lifecycle, Provenance, QueryHealth, QueryKind, Value},
    journal::{DurableStore, JournalError, OperationKind, OperationPhase, OperationRecord},
    number::TokenTotal,
    provider::{
        Cooldowns, CurrentMetric, DatasetCurrent, DatasetResult, ProviderError, ProviderPort,
        ProviderRequest, RefreshClock,
    },
    storage::{Connection, Partition, Repository, Scope, StorageError, WriteContext},
};
use quotavista_providers::{
    simulated::{Scenario, SimulatedAdapter, TERMINAL_CURSOR},
    synthetic_providers,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

type TestCoordinator =
    RefreshCoordinator<SqliteRepository, SimulatedAdapter, FakeDurable, ManualClock>;

const CONN: &str = "synthetic-connection-0";
const SCOPE_ID: &str = "synthetic-scope-synthetic-connection-0";

/// Fake clock so due/cooldown/expiry decisions are deterministic.
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

    fn advance(&self, minutes: i64) {
        *self.state.lock().unwrap() += chrono::Duration::minutes(minutes);
    }
}

impl RefreshClock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.state.lock().unwrap()
    }
}

/// In-memory durable store for cooldown persistence checks.
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

struct Harness {
    _dir: TempDir,
    repo: SqliteRepository,
    durable: FakeDurable,
    clock: ManualClock,
    adapter: SimulatedAdapter,
}

impl Harness {
    fn coordinator(&self) -> TestCoordinator {
        RefreshCoordinator::open(
            self.repo.clone(),
            self.adapter.clone(),
            self.durable.clone(),
            self.clock.clone(),
        )
        .unwrap()
    }

    async fn set_refresh_minutes(&self, minutes: Option<u8>) {
        let mut settings = self.repo.settings().await.unwrap();
        settings.refresh_minutes = minutes;
        self.repo.save_settings(&settings).await.unwrap();
    }

    fn set_scenario(&self, scenario: Scenario) {
        self.adapter.set_scenario(scenario);
    }

    /// The storage-shaped partition the coordinator stages history under.
    fn storage_partition(&self, as_of: DateTime<Utc>) -> Partition {
        let partition = self.adapter.token_partition(&request_at(as_of));
        Partition {
            scope_id: SCOPE_ID.into(),
            product_key: partition.product_key.clone(),
            dataset: partition.dataset.clone(),
            metric: partition.metric.clone(),
            interval: partition.interval,
            grain: partition.grain.clone(),
            dimensions: partition.dimensions.clone(),
            authority: partition.authority,
        }
    }
}

async fn fixture() -> Harness {
    fixture_with(1).await
}

async fn fixture_with(connections: usize) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let repo = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    for index in 0..connections {
        let id = format!("synthetic-connection-{index}");
        repo.create_connection(&connection(&id), &scope(&id))
            .await
            .unwrap();
    }
    let adapter = SimulatedAdapter::new(synthetic_providers()[0]);
    Harness {
        _dir: dir,
        repo,
        durable: FakeDurable::default(),
        clock: ManualClock::at(at(12)),
        adapter,
    }
}

fn at(hour: u32) -> DateTime<Utc> {
    format!("2026-01-01T{hour:02}:00:00Z").parse().unwrap()
}

fn request_at(as_of: DateTime<Utc>) -> ProviderRequest {
    ProviderRequest {
        connection_id: CONN.into(),
        as_of,
    }
}

fn connection(id: &str) -> Connection {
    Connection {
        id: id.into(),
        provider: format!("synthetic-provider-{id}"),
        alias: format!("Synthetic {id}"),
        generation: 0,
        lifecycle: Lifecycle::Active,
    }
}

fn scope(connection_id: &str) -> Scope {
    Scope {
        id: format!("synthetic-scope-{connection_id}"),
        connection_id: connection_id.into(),
        key: format!("synthetic-account-{connection_id}"),
        kind: "account".into(),
    }
}

fn context(connection_id: &str, generation: i64) -> WriteContext {
    WriteContext {
        connection_id: connection_id.into(),
        generation,
    }
}

/// Run a job to its terminal outcome.
async fn run_to_completion<P: ProviderPort>(
    coordinator: &mut RefreshCoordinator<SqliteRepository, P, FakeDurable, ManualClock>,
    job: &str,
) -> Outcome {
    loop {
        if let Some(outcome) = coordinator.outcome(job) {
            return outcome;
        }
        coordinator.pump().await;
    }
}

async fn admitted<P: ProviderPort>(
    coordinator: &mut RefreshCoordinator<SqliteRepository, P, FakeDurable, ManualClock>,
    conn: &str,
) -> String {
    let admissions = coordinator
        .request_manual(&[(conn, context(conn, 0))])
        .await;
    let Admission::Admitted { job } = admissions[0].clone() else {
        panic!("request for {conn} must be admitted: {admissions:?}");
    };
    job
}

#[tokio::test]
async fn manual_refresh_publishes_history_and_commits_current() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    let job = admitted(&mut coordinator, CONN).await;
    let outcome = run_to_completion(&mut coordinator, &job).await;
    assert!(matches!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Published { .. }
        }
    ));

    // History: the three synthetic UTC-day facts are atomically published.
    let published = harness
        .repo
        .published(&harness.storage_partition(at(12)))
        .await
        .unwrap()
        .expect("nominal history must publish");
    assert_eq!(published.facts.len(), 3);

    // Current: token and spend snapshots are committed with synthetic
    // provenance, and both datasets report success.
    let token = harness
        .repo
        .snapshot(SCOPE_ID, None, "tokens")
        .await
        .unwrap()
        .expect("token snapshot committed");
    assert_eq!(token.snapshot.provenance, Provenance::Synthetic);
    let money = harness
        .repo
        .snapshot(SCOPE_ID, None, "spend")
        .await
        .unwrap()
        .expect("money snapshot committed");
    assert!(matches!(money.snapshot.value, Value::Money(_)));
    assert_eq!(
        coordinator
            .health()
            .get(&(CONN.into(), "shared-pool".into())),
        Some(&QueryHealth::Success)
    );
    assert_eq!(
        coordinator.health().get(&(CONN.into(), "topups".into())),
        Some(&QueryHealth::Success)
    );
}

#[tokio::test]
async fn manual_requests_coalesce_and_respect_persisted_cooldowns() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    let job = admitted(&mut coordinator, CONN).await;
    let second = coordinator
        .request_manual(&[(CONN, context(CONN, 0))])
        .await;
    assert_eq!(
        second[0],
        Admission::Coalesced { job: job.clone() },
        "a second request for a live connection must coalesce"
    );
    assert_eq!(coordinator.job_ids(), vec![job.clone()]);

    // Cancel, then drive a rate-limited run: the retry window is persisted
    // before it is honored, and the outcome is a cooldown.
    coordinator.cancel(&job).unwrap();
    harness.set_scenario(Scenario::RateLimited);
    let job = admitted(&mut coordinator, CONN).await;
    let outcome = run_to_completion(&mut coordinator, &job).await;
    let Outcome::Cooldown { until } = outcome else {
        panic!("rate limit must end in cooldown: {outcome:?}");
    };
    assert_eq!(until, at(12) + chrono::Duration::minutes(30));
    // Whole-call transport failures record health under the empty dataset key.
    assert_eq!(
        coordinator.health().get(&(CONN.to_owned(), String::new())),
        Some(&QueryHealth::RateLimited)
    );

    let persisted = harness
        .durable
        .entry(COOLDOWNS_NAME)
        .expect("cooldown boundary must be persisted");
    let decoded = Cooldowns::decode(&persisted).unwrap();
    assert!(decoded
        .blocked(CONN, at(12) + chrono::Duration::minutes(29))
        .is_some());
    assert!(decoded
        .blocked(CONN, at(12) + chrono::Duration::minutes(30))
        .is_none());

    // While blocked, manual requests report the persisted boundary.
    let blocked = coordinator
        .request_manual(&[(CONN, context(CONN, 0))])
        .await;
    assert_eq!(
        blocked[0],
        Admission::CooldownBlocked {
            until: at(12) + chrono::Duration::minutes(30)
        }
    );

    // Once the boundary expires, admission resumes.
    harness.clock.advance(31);
    harness.set_scenario(Scenario::Nominal);
    let resumed = coordinator
        .request_manual(&[(CONN, context(CONN, 0))])
        .await;
    assert!(matches!(resumed[0], Admission::Admitted { .. }));
}

#[tokio::test]
async fn automatic_refresh_off_launches_nothing_and_on_respects_interval() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    // Automatic refresh off: no job, no query.
    harness.set_refresh_minutes(None).await;
    let admissions = coordinator.tick(&[(CONN, context(CONN, 0))]).await;
    assert_eq!(admissions[0], Admission::Refused);
    assert!(coordinator.job_ids().is_empty());
    assert_eq!(coordinator.health().len(), 0);

    // On with a five-minute interval: admitted, completes, not due again
    // until the interval elapses.
    harness.set_refresh_minutes(Some(5)).await;
    let admissions = coordinator.tick(&[(CONN, context(CONN, 0))]).await;
    let Admission::Admitted { job } = admissions[0].clone() else {
        panic!("due connection must be admitted: {admissions:?}");
    };
    let outcome = run_to_completion(&mut coordinator, &job).await;
    assert!(matches!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Published { .. }
        }
    ));

    let too_soon = coordinator.tick(&[(CONN, context(CONN, 0))]).await;
    assert_eq!(too_soon[0], Admission::Refused);

    harness.clock.advance(6);
    let due = coordinator.tick(&[(CONN, context(CONN, 0))]).await;
    assert!(matches!(due[0], Admission::Admitted { .. }));
}

#[tokio::test]
async fn concurrency_is_two_and_cancellation_frees_the_slot() {
    let harness = fixture_with(3).await;
    let mut coordinator = harness.coordinator();

    let ids: Vec<String> = (0..3)
        .map(|i| format!("synthetic-connection-{i}"))
        .collect();
    let requests: Vec<(&str, WriteContext)> =
        ids.iter().map(|id| (id.as_str(), context(id, 0))).collect();
    let admissions = coordinator.request_manual(&requests).await;
    for admission in &admissions {
        assert!(matches!(admission, Admission::Admitted { .. }));
    }
    assert_eq!(coordinator.running(), 2);
    assert_eq!(coordinator.pending(), 1);

    // Cancel a running job: its slot frees and the pending job is promoted.
    let live: Vec<String> = coordinator
        .job_ids()
        .into_iter()
        .filter(|id| coordinator.outcome(id).is_none())
        .collect();
    assert_eq!(coordinator.cancel(&live[0]), Some(Outcome::Cancelled));
    assert_eq!(coordinator.running(), 1);
    assert_eq!(coordinator.pending(), 1);

    coordinator.pump().await;
    assert_eq!(coordinator.running(), 2);
    assert_eq!(coordinator.pending(), 0);

    let cancelled = coordinator.outcome(&live[0]).unwrap();
    assert_eq!(cancelled, Outcome::Cancelled);
    for id in coordinator.job_ids() {
        run_to_completion(&mut coordinator, &id).await;
    }
    assert_eq!(coordinator.running(), 0);
}

#[tokio::test]
async fn failing_dataset_keeps_committed_values_and_errors_isolated() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    // First run commits the token snapshot.
    let job = admitted(&mut coordinator, CONN).await;
    run_to_completion(&mut coordinator, &job).await;

    // Second run denies the token dataset only: the committed value survives
    // and the money dataset still reports success alongside the token error.
    harness.set_scenario(Scenario::PartialPermission);
    let job = admitted(&mut coordinator, CONN).await;
    let outcome = run_to_completion(&mut coordinator, &job).await;
    assert!(matches!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Published { .. }
        }
    ));

    let retained = harness
        .repo
        .snapshot(SCOPE_ID, None, "tokens")
        .await
        .unwrap()
        .expect("committed token snapshot must survive a failed refresh");
    assert_eq!(
        retained.snapshot.value,
        Value::Tokens(TokenTotal(1_234_567_890_123))
    );

    assert_eq!(
        coordinator
            .health()
            .get(&(CONN.into(), "shared-pool".into())),
        Some(&QueryHealth::PermissionRequired)
    );
    assert_eq!(
        coordinator.health().get(&(CONN.into(), "topups".into())),
        Some(&QueryHealth::Success)
    );
}

#[tokio::test]
async fn late_responses_are_fenced_after_disable_and_deletion() {
    for lifecycle in [Lifecycle::Disabled, Lifecycle::DeletionPending] {
        let harness = fixture().await;
        let mut coordinator = harness.coordinator();

        let job = admitted(&mut coordinator, CONN).await;

        // The job captured generation 0 and has queried the provider; the
        // lifecycle change bumps the generation, so the late write fences.
        coordinator.pump().await; // QueryCurrent -> CommitCurrent
        let bumped = harness
            .repo
            .set_lifecycle(&context(CONN, 0), lifecycle)
            .await
            .unwrap();
        assert_eq!(bumped, 1);

        let outcome = run_to_completion(&mut coordinator, &job).await;
        assert_eq!(
            outcome,
            Outcome::Fenced,
            "late write after {lifecycle:?} must be fenced"
        );
    }
}

#[tokio::test]
async fn incomplete_history_keeps_published_partition_untouched() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    harness.set_scenario(Scenario::IncompleteHistory);
    let job = admitted(&mut coordinator, CONN).await;
    let outcome = run_to_completion(&mut coordinator, &job).await;
    assert_eq!(
        outcome,
        Outcome::Completed {
            history: HistoryOutcome::Incomplete
        }
    );

    // Partial history never publishes: the published partition stays absent
    // while the staged candidate is retained at its last synthetic cursor.
    let published = harness
        .repo
        .published(&harness.storage_partition(at(12)))
        .await
        .unwrap();
    assert!(published.is_none(), "incomplete history must not publish");

    let cursor = harness.repo.candidate_cursor(&job).await.unwrap();
    assert_eq!(cursor.as_deref(), Some("synthetic-page-100"));
    assert_ne!(cursor.as_deref(), Some(TERMINAL_CURSOR));
}

#[tokio::test]
async fn adapter_partition_shape_is_shared_with_storage() {
    // Guard: the test helper maps the adapter partition faithfully.
    let harness = fixture().await;
    let partition = harness.storage_partition(at(12));
    let adapter = harness.adapter.token_partition(&request_at(at(12)));
    assert_eq!(partition.dataset, adapter.dataset);
    assert_eq!(partition.metric, adapter.metric);
    assert_eq!(partition.interval, adapter.interval);
    assert_eq!(partition.authority, Provenance::Synthetic);
}

#[tokio::test]
async fn a_second_coordinator_publishes_after_a_restart_on_the_same_database() {
    let harness = fixture().await;

    let mut first = harness.coordinator();
    let job_one = admitted(&mut first, CONN).await;
    let Outcome::Completed {
        history: HistoryOutcome::Published {
            revision: first_revision,
        },
    } = run_to_completion(&mut first, &job_one).await
    else {
        panic!("first coordinator must publish");
    };

    // A coordinator reopened on the same database and durable store issues
    // fresh job ids and must still publish.
    let mut second = harness.coordinator();
    let job_two = admitted(&mut second, CONN).await;
    assert_ne!(
        job_one, job_two,
        "job ids must never be reused across restarts"
    );
    let outcome = run_to_completion(&mut second, &job_two).await;
    let Outcome::Completed {
        history: HistoryOutcome::Published { revision },
    } = outcome
    else {
        panic!("restart must publish, got {outcome:?}");
    };
    assert!(revision > first_revision);
}

#[tokio::test]
async fn disabled_and_deletion_pending_connections_are_not_admitted() {
    for lifecycle in [Lifecycle::Disabled, Lifecycle::DeletionPending] {
        let harness = fixture().await;
        harness
            .repo
            .set_lifecycle(&context(CONN, 0), lifecycle)
            .await
            .unwrap();
        let mut coordinator = harness.coordinator();
        let admissions = coordinator
            .request_manual(&[(CONN, context(CONN, 0))])
            .await;
        assert_eq!(admissions[0], Admission::Refused, "{lifecycle:?}");
        assert!(coordinator.job_ids().is_empty());
        assert_eq!(coordinator.running(), 0);
    }
}

#[tokio::test]
async fn a_late_rate_limit_after_disable_is_fenced_without_a_cooldown() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    let job = admitted(&mut coordinator, CONN).await;
    coordinator.pump().await; // QueryCurrent -> CommitCurrent
    coordinator.pump().await; // CommitCurrent -> FetchPage

    // The provider starts rate limiting only now and the connection is
    // disabled: the late error fences without any durable side effect.
    harness.set_scenario(Scenario::RateLimited);
    harness
        .repo
        .set_lifecycle(&context(CONN, 0), Lifecycle::Disabled)
        .await
        .unwrap();

    let outcome = run_to_completion(&mut coordinator, &job).await;
    assert_eq!(outcome, Outcome::Fenced);
    assert!(harness.durable.entry(COOLDOWNS_NAME).is_none());
    assert!(
        coordinator
            .health()
            .get(&(CONN.to_owned(), String::new()))
            .is_none(),
        "no health entry for a dead context"
    );
}

#[tokio::test]
async fn late_response_is_fenced_after_credential_rotation() {
    let harness = fixture().await;
    let mut coordinator = harness.coordinator();

    let job = admitted(&mut coordinator, CONN).await;
    coordinator.pump().await; // QueryCurrent -> CommitCurrent

    // Credential rotation commits a new reference and bumps the generation
    // while the connection stays Active.
    let operation = OperationRecord {
        operation_id: "synthetic-rotation".into(),
        connection_id: CONN.into(),
        kind: OperationKind::Setup,
        phase: OperationPhase::Pending,
        candidate_ref: Some("synthetic-ref-next".into()),
        previous_ref: None,
        generation: 0,
    };
    harness.repo.begin_operation(&operation).await.unwrap();
    let bumped = harness
        .repo
        .activate_reference(
            &context(CONN, 0),
            "synthetic-rotation",
            "synthetic-ref-next",
            None,
        )
        .await
        .unwrap();
    assert_eq!(bumped, 1);
    let row = harness.repo.connection(CONN).await.unwrap().unwrap();
    assert_eq!(row.lifecycle, Lifecycle::Active);

    let outcome = run_to_completion(&mut coordinator, &job).await;
    assert_eq!(outcome, Outcome::Fenced);
}

#[tokio::test]
async fn no_history_completes_without_a_published_partition() {
    let harness = fixture().await;
    harness.set_scenario(Scenario::NoHistory);
    let mut coordinator = harness.coordinator();

    let job = admitted(&mut coordinator, CONN).await;
    assert_eq!(
        run_to_completion(&mut coordinator, &job).await,
        Outcome::Completed {
            history: HistoryOutcome::Skipped
        }
    );
    // Current state still commits; nothing is published for history.
    assert!(harness
        .repo
        .snapshot(SCOPE_ID, None, "tokens")
        .await
        .unwrap()
        .is_some());
    let partition = harness.storage_partition(at(12));
    assert!(harness.repo.published(&partition).await.unwrap().is_none());
}

#[tokio::test]
async fn a_withdrawn_terminal_attestation_does_not_publish() {
    let harness = fixture().await;
    harness.set_scenario(Scenario::PartialAttestation);
    let mut coordinator = harness.coordinator();

    let job = admitted(&mut coordinator, CONN).await;
    assert_eq!(
        run_to_completion(&mut coordinator, &job).await,
        Outcome::Completed {
            history: HistoryOutcome::Incomplete
        }
    );
    let partition = harness.storage_partition(at(12));
    assert!(harness.repo.published(&partition).await.unwrap().is_none());
    assert_eq!(
        harness.repo.candidate_cursor(&job).await.unwrap(),
        Some(END_CURSOR.into()),
        "partial staging stays separate from published totals"
    );
}

/// Provider stub whose current-state metrics fail validation, to exercise the
/// commit fence without relying on an adapter scenario.
struct InvalidMetricsAdapter(SimulatedAdapter);

impl ProviderPort for InvalidMetricsAdapter {
    async fn current(
        &self,
        request: &ProviderRequest,
    ) -> Result<Vec<DatasetCurrent>, ProviderError> {
        let inconsistent = CurrentMetric {
            dataset: "shared-pool".into(),
            metric: "tokens".into(),
            product_key: None,
            // A present value without Available availability cannot validate.
            value: Some(Value::Tokens(TokenTotal(1))),
            availability: Availability::Missing,
            reason: None,
            query_kind: QueryKind::Unvalidated,
            provenance: Provenance::Synthetic,
            observed_at: request.as_of,
            source_as_of: Some(request.as_of),
        };
        Ok(vec![DatasetCurrent {
            dataset: "shared-pool".into(),
            result: DatasetResult::Metrics(vec![inconsistent]),
        }])
    }

    async fn history_page(
        &self,
        request: &ProviderRequest,
        cursor: Option<&str>,
    ) -> Result<Option<HistoryPage>, ProviderError> {
        self.0.history_page(request, cursor).await
    }
}

/// Kill the admission context between the current fetch and the commit:
/// rotation bumps the generation while staying Active, disable changes the
/// lifecycle instead.
async fn kill_context(harness: &Harness, rotate: bool) {
    if rotate {
        let operation = OperationRecord {
            operation_id: "synthetic-rotation".into(),
            connection_id: CONN.into(),
            kind: OperationKind::Setup,
            phase: OperationPhase::Pending,
            candidate_ref: Some("synthetic-ref-next".into()),
            previous_ref: None,
            generation: 0,
        };
        harness.repo.begin_operation(&operation).await.unwrap();
        harness
            .repo
            .activate_reference(
                &context(CONN, 0),
                "synthetic-rotation",
                "synthetic-ref-next",
                None,
            )
            .await
            .unwrap();
    } else {
        harness
            .repo
            .set_lifecycle(&context(CONN, 0), Lifecycle::Disabled)
            .await
            .unwrap();
    }
}

/// The dead-context contract: no health entry anywhere for the connection
/// and no history run ever begun.
async fn assert_dead_context<P: ProviderPort>(
    coordinator: &RefreshCoordinator<SqliteRepository, P, FakeDurable, ManualClock>,
    harness: &Harness,
    job: &str,
) {
    let health = coordinator.health();
    assert!(
        !health.contains_key(&(CONN.into(), "shared-pool".into())),
        "token dataset health must not be recorded: {health:?}"
    );
    assert!(
        !health.contains_key(&(CONN.into(), "topups".into())),
        "money dataset health must not be recorded: {health:?}"
    );
    assert!(
        !health.contains_key(&(CONN.into(), String::new())),
        "whole-call health must not be recorded: {health:?}"
    );
    assert!(
        matches!(
            harness.repo.candidate_cursor(job).await,
            Err(StorageError::Conflict)
        ),
        "history must never begin for a dead context"
    );
}

#[tokio::test]
async fn a_partial_permission_commit_for_a_dead_context_is_fenced() {
    for rotate in [false, true] {
        let harness = fixture().await;
        harness.set_scenario(Scenario::PartialPermission);
        let mut coordinator = harness.coordinator();

        let job = admitted(&mut coordinator, CONN).await;
        coordinator.pump().await; // QueryCurrent -> CommitCurrent
        kill_context(&harness, rotate).await;

        let outcome = run_to_completion(&mut coordinator, &job).await;
        assert!(matches!(outcome, Outcome::Fenced), "{outcome:?}");
        assert_dead_context(&coordinator, &harness, &job).await;
    }
}

#[tokio::test]
async fn an_invalid_metric_commit_for_a_dead_context_is_fenced() {
    for rotate in [false, true] {
        let harness = fixture().await;
        let mut coordinator = RefreshCoordinator::open(
            harness.repo.clone(),
            InvalidMetricsAdapter(harness.adapter.clone()),
            harness.durable.clone(),
            harness.clock.clone(),
        )
        .unwrap();

        let job = admitted(&mut coordinator, CONN).await;
        coordinator.pump().await; // QueryCurrent -> CommitCurrent
        kill_context(&harness, rotate).await;

        let outcome = run_to_completion(&mut coordinator, &job).await;
        assert!(matches!(outcome, Outcome::Fenced), "{outcome:?}");
        assert_dead_context(&coordinator, &harness, &job).await;
    }
}
