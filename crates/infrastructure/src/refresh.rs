//! Deterministic refresh coordination over provider ports and the repository.
//!
//! The coordinator owns scheduling; adapters only answer queries. Jobs advance
//! one provider-or-repository call per [`RefreshCoordinator::pump`] so tests
//! interleave disable, cancel, and clock changes deterministically. A job
//! captures its [`WriteContext`] at admission: a later disable or delete bumps
//! the generation, and the repository fence rejects the late write as
//! [`Outcome::Fenced`] instead of letting a stale response land.

use chrono::{DateTime, Duration, Utc};
use quotavista_core::{
    domain::QueryHealth,
    journal::{DurableStore, JournalError, COOLDOWNS_NAME},
    provider::{
        Cooldowns, DatasetCurrent, DatasetResult, HistoryPage, ProviderError, ProviderPort,
        ProviderRequest, RefreshClock,
    },
    storage::{Partition, Repository, Snapshot, StorageError, WriteContext},
};
use std::collections::BTreeMap;

/// At most two sync jobs run at once, however many are admitted.
pub const MAX_CONCURRENCY: usize = 2;
/// History slice bounds from the architecture: page count and wall-clock time.
pub const MAX_HISTORY_PAGES: u32 = 100;
const MAX_HISTORY_SECONDS: i64 = 30;
/// Staging cursor synthesized for a provider's final page (no continuation).
const END_CURSOR: &str = "history-end";

/// What happened to one connection's refresh request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Admission {
    Admitted { job: String },
    Coalesced { job: String },
    CooldownBlocked { until: DateTime<Utc> },
    Refused,
}

/// Quality of a finished job's history stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryOutcome {
    /// Complete, settled coverage replaced the partition atomically.
    Published { revision: i64 },
    /// Partial staging only; the published partition is untouched and the
    /// stream never attested completeness.
    Incomplete,
    /// The provider serves no history for this connection.
    Skipped,
}

/// Terminal state of one job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Completed {
        history: HistoryOutcome,
    },
    Cancelled,
    /// A late write was rejected by the lifecycle/generation fence.
    Fenced,
    /// The provider's retry window was persisted; the connection is blocked
    /// until `until`.
    Cooldown {
        until: DateTime<Utc>,
    },
    Failed {
        error: ProviderError,
    },
    /// A local repository write failed for a non-fencing reason.
    StorageFailed,
}

/// One step of the job state machine. Each variant's `advance` performs at
/// most one awaitable call.
enum Phase {
    /// Admitted, waiting for a free concurrency slot.
    Pending,
    QueryCurrent,
    CommitCurrent {
        datasets: Vec<DatasetCurrent>,
    },
    FetchPage {
        cursor: Option<String>,
        fetched: u32,
    },
    BeginCandidate {
        page: HistoryPage,
    },
    StagePage {
        page: HistoryPage,
        number: u32,
    },
    CompleteHistory {
        pages: u32,
    },
    Publish {
        pages: u32,
    },
}

struct Job {
    id: String,
    connection_id: String,
    context: WriteContext,
    scope_id: String,
    started_at: DateTime<Utc>,
    as_of: Option<DateTime<Utc>>,
    phase: Phase,
    staged_cursor: Option<String>,
    saw_complete: bool,
    outcome: Option<Outcome>,
}

/// Schedules and drives refresh jobs for provider connections.
pub struct RefreshCoordinator<R, P, D, C> {
    repository: R,
    provider: P,
    durable: D,
    clock: C,
    jobs: Vec<Option<Job>>,
    next_job: u64,
    cooldowns: Cooldowns,
    health: BTreeMap<(String, String), QueryHealth>,
    /// Last completion per connection, in memory only: a restart re-admits on
    /// the next tick rather than trusting unproven elapsed time.
    last_completion: BTreeMap<String, DateTime<Utc>>,
}

impl<R: Repository, P: ProviderPort, D: DurableStore, C: RefreshClock>
    RefreshCoordinator<R, P, D, C>
{
    /// Load persisted cooldowns and open the coordinator. Unreadable cooldown
    /// state is a visible error, never silently ignored.
    pub fn open(repository: R, provider: P, durable: D, clock: C) -> Result<Self, JournalError> {
        let cooldowns = quotavista_core::provider::load_cooldowns(&durable)?;
        Ok(Self {
            repository,
            provider,
            durable,
            clock,
            jobs: Vec::new(),
            next_job: 0,
            cooldowns,
            health: BTreeMap::new(),
            last_completion: BTreeMap::new(),
        })
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    pub fn durable(&self) -> &D {
        &self.durable
    }

    /// Terminal outcome of a job, or `None` while it still runs or waits.
    pub fn outcome(&self, job_id: &str) -> Option<Outcome> {
        self.jobs
            .iter()
            .flatten()
            .find(|job| job.id == job_id)
            .and_then(|job| job.outcome.clone())
    }

    pub fn job_ids(&self) -> Vec<String> {
        self.jobs
            .iter()
            .flatten()
            .map(|job| job.id.clone())
            .collect()
    }

    /// Jobs currently occupying a concurrency slot.
    pub fn running(&self) -> usize {
        self.jobs
            .iter()
            .flatten()
            .filter(|job| job.outcome.is_none() && !matches!(job.phase, Phase::Pending))
            .count()
    }

    /// Admitted jobs still waiting for a slot.
    pub fn pending(&self) -> usize {
        self.jobs
            .iter()
            .flatten()
            .filter(|job| job.outcome.is_none() && matches!(job.phase, Phase::Pending))
            .count()
    }

    /// Last recorded query health per `(connection, dataset)`. A failing
    /// dataset updates only its own entry.
    pub fn health(&self) -> &BTreeMap<(String, String), QueryHealth> {
        &self.health
    }

    /// Explicit user refresh. Coalesces into a live job for the same
    /// connection and respects persisted provider cooldowns.
    pub async fn request_manual(&mut self, requests: &[(&str, WriteContext)]) -> Vec<Admission> {
        let now = self.clock.now();
        let mut admissions = Vec::with_capacity(requests.len());
        for (connection_id, context) in requests {
            admissions.push(
                self.admit(connection_id, context, now, /* automatic = */ false)
                    .await,
            );
        }
        admissions
    }

    /// Scheduled refresh. With automatic refresh off (`refresh_minutes` is
    /// `None`) nothing is launched and no query is issued.
    pub async fn tick(&mut self, requests: &[(&str, WriteContext)]) -> Vec<Admission> {
        let minutes = match self.repository.settings().await {
            Ok(settings) => settings.refresh_minutes,
            Err(_) => None,
        };
        let now = self.clock.now();
        let mut admissions = Vec::with_capacity(requests.len());
        for (connection_id, context) in requests {
            let admission = match minutes {
                None => Admission::Refused,
                Some(minutes) => {
                    let due = self
                        .last_completion
                        .get(*connection_id)
                        .is_none_or(|last| now >= *last + Duration::minutes(i64::from(minutes)));
                    if due {
                        self.admit(connection_id, context, now, /* automatic = */ true)
                            .await
                    } else {
                        Admission::Refused
                    }
                }
            };
            admissions.push(admission);
        }
        admissions
    }

    /// Cancel a job. The slot frees immediately; a provider call already in
    /// flight inside `pump` is unaffected until its step returns.
    pub fn cancel(&mut self, job_id: &str) -> Option<Outcome> {
        let job = self
            .jobs
            .iter_mut()
            .flatten()
            .find(|job| job.id == job_id && job.outcome.is_none())?;
        job.outcome = Some(Outcome::Cancelled);
        Some(Outcome::Cancelled)
    }

    /// Advance every non-pending job by exactly one awaitable call, promoting
    /// pending jobs into free slots first.
    pub async fn pump(&mut self) {
        let mut running = self.running();
        for slot in self.jobs.iter_mut().flatten() {
            if running >= MAX_CONCURRENCY {
                break;
            }
            if slot.outcome.is_none() && matches!(slot.phase, Phase::Pending) {
                slot.phase = Phase::QueryCurrent;
                running += 1;
            }
        }
        for index in 0..self.jobs.len() {
            let Some(mut job) = self.jobs[index].take() else {
                continue;
            };
            if job.outcome.is_none() && !matches!(job.phase, Phase::Pending) {
                self.advance(&mut job).await;
            }
            self.jobs[index] = Some(job);
        }
    }

    async fn admit(
        &mut self,
        connection_id: &str,
        context: &WriteContext,
        now: DateTime<Utc>,
        automatic: bool,
    ) -> Admission {
        if let Some(job) = self
            .jobs
            .iter()
            .flatten()
            .find(|job| job.outcome.is_none() && job.connection_id == connection_id)
        {
            return Admission::Coalesced {
                job: job.id.clone(),
            };
        }
        if let Some(until) = self.cooldowns.blocked(connection_id, now) {
            return Admission::CooldownBlocked { until };
        }
        // Automatic refresh silently skips blocked or unknown connections;
        // manual requests report the refusal.
        if self
            .repository
            .connection(connection_id)
            .await
            .ok()
            .flatten()
            .is_none()
        {
            return Admission::Refused;
        }
        let Ok(scopes) = self.repository.scope_ids(connection_id).await else {
            return Admission::Refused;
        };
        let Some(scope_id) = scopes.first().cloned() else {
            return Admission::Refused;
        };
        let _ = automatic;
        let id = format!("job-{}", self.next_job);
        self.next_job += 1;
        let phase = if self.running() < MAX_CONCURRENCY {
            Phase::QueryCurrent
        } else {
            Phase::Pending
        };
        self.jobs.push(Some(Job {
            id: id.clone(),
            connection_id: connection_id.to_owned(),
            context: context.clone(),
            scope_id,
            started_at: now,
            as_of: None,
            phase,
            staged_cursor: None,
            saw_complete: false,
            outcome: None,
        }));
        Admission::Admitted { job: id }
    }

    async fn advance(&mut self, job: &mut Job) {
        match std::mem::replace(&mut job.phase, Phase::Pending) {
            Phase::Pending => {}
            Phase::QueryCurrent => {
                let as_of = self.clock.now();
                job.as_of = Some(as_of);
                let request = ProviderRequest {
                    connection_id: job.connection_id.clone(),
                    as_of,
                };
                match self.provider.current(&request).await {
                    Ok(datasets) => job.phase = Phase::CommitCurrent { datasets },
                    Err(error) => self.fail_provider(job, error, as_of),
                }
            }
            Phase::CommitCurrent { datasets } => {
                self.commit_current(job, datasets).await;
            }
            Phase::FetchPage { cursor, fetched } => {
                let now = self.clock.now();
                if fetched >= MAX_HISTORY_PAGES
                    || now - job.started_at > Duration::seconds(MAX_HISTORY_SECONDS)
                {
                    job.outcome = Some(Outcome::Completed {
                        history: HistoryOutcome::Incomplete,
                    });
                    return;
                }
                let request = ProviderRequest {
                    connection_id: job.connection_id.clone(),
                    as_of: job.as_of.unwrap_or(now),
                };
                match self
                    .provider
                    .history_page(&request, cursor.as_deref())
                    .await
                {
                    Err(error) => self.fail_provider(job, error, now),
                    Ok(None) => {
                        job.phase = if job.saw_complete {
                            Phase::CompleteHistory { pages: fetched }
                        } else if fetched == 0 {
                            unreachable!("provider ended the stream before any page")
                        } else {
                            job.outcome = Some(Outcome::Completed {
                                history: HistoryOutcome::Incomplete,
                            });
                            Phase::Pending
                        };
                    }
                    Ok(Some(page)) => {
                        job.saw_complete |= page.complete;
                        if fetched == 0 {
                            job.phase = Phase::BeginCandidate { page };
                        } else {
                            job.phase = Phase::StagePage {
                                page,
                                number: fetched + 1,
                            };
                        }
                    }
                }
            }
            Phase::BeginCandidate { page } => {
                let partition = Partition {
                    scope_id: job.scope_id.clone(),
                    product_key: page.partition.product_key.clone(),
                    dataset: page.partition.dataset.clone(),
                    metric: page.partition.metric.clone(),
                    interval: page.partition.interval,
                    grain: page.partition.grain.clone(),
                    dimensions: page.partition.dimensions.clone(),
                    authority: page.partition.authority,
                };
                match self
                    .repository
                    .begin_candidate(&job.context, &job.id, &partition)
                    .await
                {
                    Ok(()) => job.phase = Phase::StagePage { page, number: 1 },
                    Err(StorageError::Conflict) => job.outcome = Some(Outcome::Fenced),
                    Err(_) => job.outcome = Some(Outcome::StorageFailed),
                }
            }
            Phase::StagePage { page, number } => {
                let continuation = page.next_cursor.clone();
                let next_cursor = continuation
                    .clone()
                    .unwrap_or_else(|| END_CURSOR.to_owned());
                match self
                    .repository
                    .stage_page(
                        &job.context,
                        &job.id,
                        job.staged_cursor.as_deref(),
                        &next_cursor,
                        &page.facts,
                    )
                    .await
                {
                    Err(StorageError::Conflict) => job.outcome = Some(Outcome::Fenced),
                    Err(_) => job.outcome = Some(Outcome::StorageFailed),
                    Ok(()) => {
                        job.staged_cursor = Some(next_cursor);
                        match continuation {
                            Some(cursor) => {
                                job.phase = Phase::FetchPage {
                                    cursor: Some(cursor),
                                    fetched: number,
                                };
                            }
                            None => {
                                job.phase = if job.saw_complete {
                                    Phase::CompleteHistory { pages: number }
                                } else {
                                    job.outcome = Some(Outcome::Completed {
                                        history: HistoryOutcome::Incomplete,
                                    });
                                    Phase::Pending
                                };
                            }
                        }
                    }
                }
            }
            Phase::CompleteHistory { pages } => {
                match self
                    .repository
                    .complete_candidate(&job.context, &job.id, job.staged_cursor.as_deref())
                    .await
                {
                    Ok(()) => job.phase = Phase::Publish { pages },
                    Err(StorageError::Conflict) => job.outcome = Some(Outcome::Fenced),
                    Err(_) => job.outcome = Some(Outcome::StorageFailed),
                }
            }
            Phase::Publish { pages } => {
                let succeeded_at = job.as_of.unwrap_or_else(|| self.clock.now());
                let watermark = format!("{}:{}", job.id, pages);
                match self
                    .repository
                    .publish(&job.context, &job.id, succeeded_at, &watermark)
                    .await
                {
                    Ok(revision) => {
                        self.last_completion
                            .insert(job.connection_id.clone(), succeeded_at);
                        job.outcome = Some(Outcome::Completed {
                            history: HistoryOutcome::Published { revision },
                        });
                    }
                    Err(StorageError::Conflict) => job.outcome = Some(Outcome::Fenced),
                    Err(_) => job.outcome = Some(Outcome::StorageFailed),
                }
            }
        }
    }

    /// Persist committed current values and per-dataset health. A failing
    /// dataset keeps its committed values and its own error; healthy datasets
    /// still commit.
    async fn commit_current(&mut self, job: &mut Job, datasets: Vec<DatasetCurrent>) {
        let as_of = job.as_of.unwrap_or_else(|| self.clock.now());
        let mut rate_limited: Option<Duration> = None;
        for dataset in datasets {
            let key = (job.connection_id.clone(), dataset.dataset.clone());
            match dataset.result {
                DatasetResult::Metrics(metrics) => {
                    if !metrics.iter().all(|metric| metric.validate().is_ok()) {
                        self.health.insert(key, QueryHealth::InvalidResponse);
                        continue;
                    }
                    for metric in &metrics {
                        let Some(value) = metric.value.clone() else {
                            continue;
                        };
                        let snapshot = Snapshot {
                            scope_id: job.scope_id.clone(),
                            product_id: metric.product_key.clone(),
                            metric: metric.metric.clone(),
                            value,
                            provenance: metric.provenance,
                            observed_at: metric.observed_at,
                            source_as_of: metric.source_as_of,
                        };
                        match self
                            .repository
                            .save_snapshot(&job.context, &snapshot, as_of)
                            .await
                        {
                            Ok(_) => {}
                            Err(StorageError::Conflict) => {
                                job.outcome = Some(Outcome::Fenced);
                                return;
                            }
                            Err(_) => {
                                job.outcome = Some(Outcome::StorageFailed);
                                return;
                            }
                        }
                    }
                    self.health.insert(key, QueryHealth::Success);
                }
                DatasetResult::Failed(error) => {
                    self.health.insert(key, error.health());
                    if let Some(retry_after) = error.cooldown() {
                        rate_limited = Some(retry_after);
                    }
                }
            }
        }
        if let Some(retry_after) = rate_limited {
            self.fail_provider(job, ProviderError::RateLimited { retry_after }, as_of);
            return;
        }
        job.phase = Phase::FetchPage {
            cursor: None,
            fetched: 0,
        };
    }

    /// Terminate a job on a provider error, persisting any retry window
    /// durably before it is honored in memory.
    fn fail_provider(&mut self, job: &mut Job, error: ProviderError, now: DateTime<Utc>) {
        if let Some(retry_after) = error.cooldown() {
            let until = now + retry_after;
            let mut next = self.cooldowns.clone();
            next.extend(&job.connection_id, until, now);
            if self
                .durable
                .replace(COOLDOWNS_NAME, &next.encode())
                .is_err()
            {
                job.outcome = Some(Outcome::StorageFailed);
                return;
            }
            self.cooldowns = next;
            job.outcome = Some(Outcome::Cooldown { until });
        } else {
            job.outcome = Some(Outcome::Failed { error });
        }
    }
}

#[cfg(test)]
mod tests;
