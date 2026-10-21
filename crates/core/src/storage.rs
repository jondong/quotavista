//! Backend storage ports. No paths, SQL, transport serialization, or credentials.
use crate::{
    calendar::Interval,
    domain::{Lifecycle, Provenance, Value},
    journal::OperationRecord,
    number::Currency,
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    Validation,
    Conflict,
    Unavailable,
    Migration,
    IncompatibleVersion,
    /// A bounded read refused because the result would exceed its cap.
    ResourceLimit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    pub revision: i64,
    pub locale: String,
    pub reporting_zone: Tz,
    pub currency: Currency,
    /// None disables automatic refresh; otherwise 1, 5, or 15 minutes.
    pub refresh_minutes: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Connection {
    pub id: String,
    pub provider: String,
    pub alias: String,
    pub generation: i64,
    pub lifecycle: Lifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scope {
    pub id: String,
    pub connection_id: String,
    pub key: String,
    pub kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteContext {
    pub connection_id: String,
    pub generation: i64,
}

/// A known current value, not a query-health or alert-eligibility decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub scope_id: String,
    pub product_id: Option<String>,
    pub metric: String,
    pub value: Value,
    pub provenance: Provenance,
    pub observed_at: DateTime<Utc>,
    pub source_as_of: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentValue {
    pub snapshot: Snapshot,
    pub successful_sync_at: DateTime<Utc>,
    pub data_revision: i64,
}

/// All dimensions are canonical, non-null strings; "" means no product/dimensions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Partition {
    pub scope_id: String,
    pub product_key: String,
    pub dataset: String,
    pub metric: String,
    pub interval: Interval,
    pub grain: String,
    pub dimensions: String,
    pub authority: Provenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactIdentity {
    /// Occurrence time and source revision deliberately do not define identity.
    Event(String),
    /// Identity uses fact bounds plus the partition's grain, dimensions, and authority.
    Bucket,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fact {
    pub identity: FactIdentity,
    pub interval: Interval,
    /// Token facts are u64; snapshots/checked aggregates may use u128.
    pub value: Value,
    pub source_revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedPartition {
    pub facts: Vec<Fact>,
    pub data_revision: i64,
    pub successful_sync_at: DateTime<Utc>,
    pub watermark: String,
}

/// A persisted coverage row: the interval plus the zone its boundaries follow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageRow {
    pub interval: Interval,
    pub status: crate::domain::CoverageStatus,
    pub reason: Option<crate::domain::Reason>,
    pub zone: String,
}

/// A published partition with its facts, coverage rows, and publication metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPartition {
    pub partition: Partition,
    pub facts: Vec<Fact>,
    pub coverage: Vec<CoverageRow>,
    pub data_revision: i64,
    pub successful_sync_at: DateTime<Utc>,
    pub watermark: String,
}
/// Generic async port; the adapter owns transaction and connection management.
#[allow(async_fn_in_trait)]
pub trait Repository {
    async fn settings(&self) -> Result<Settings, StorageError>;
    async fn save_settings(&self, settings: &Settings) -> Result<Settings, StorageError>;
    async fn create_connection(
        &self,
        connection: &Connection,
        scope: &Scope,
    ) -> Result<(), StorageError>;
    async fn connection(&self, id: &str) -> Result<Option<Connection>, StorageError>;
    /// Every persisted connection, ordered by id. Used by the dispatcher
    /// to reopen state and feed the refresh coordinator with the live set.
    async fn connections(&self) -> Result<Vec<Connection>, StorageError>;
    /// The store-wide data revision. Every committed snapshot or published
    /// partition advances it, so it is the monotonic revision carried by the
    /// change events.
    async fn data_revision(&self) -> Result<i64, StorageError>;
    async fn set_lifecycle(
        &self,
        context: &WriteContext,
        lifecycle: Lifecycle,
    ) -> Result<i64, StorageError>;

    /// Opaque scope identifiers of one connection, ordered. The deletion ledger
    /// fences every one of them because a managed backup manifest lists scopes.
    async fn scope_ids(&self, connection_id: &str) -> Result<Vec<String>, StorageError>;
    /// Opaque credential reference of a connection, so deletion cleanup can
    /// remove it. `None` means the connection holds no reference; a missing
    /// connection is a conflict. This is a reference, never a secret value.
    async fn credential_ref(&self, connection_id: &str) -> Result<Option<String>, StorageError>;
    /// Insert or refresh the database projection of a journaled lifecycle
    /// operation. An incomplete projection fences result writes for the
    /// connection; the projection never replaces the external journal.
    async fn begin_operation(&self, record: &OperationRecord) -> Result<(), StorageError>;
    /// Advance an existing projection, releasing the fence when the phase is
    /// complete. Refuses to reopen a completed operation.
    async fn advance_operation(&self, record: &OperationRecord) -> Result<(), StorageError>;
    /// Journaled operation projections. `None` returns every incomplete
    /// projection across all connections, for recovery reconciliation.
    async fn operations(
        &self,
        connection_id: Option<&str>,
    ) -> Result<Vec<OperationRecord>, StorageError>;
    /// Activate a candidate secret reference and advance the operation projection
    /// in one write transaction that re-checks the generation, the lifecycle, and
    /// the mutation fence. Returns the new generation.
    async fn activate_reference(
        &self,
        context: &WriteContext,
        operation_id: &str,
        candidate_ref: &str,
        previous_ref: Option<&str>,
    ) -> Result<i64, StorageError>;
    async fn save_snapshot(
        &self,
        context: &WriteContext,
        snapshot: &Snapshot,
        succeeded_at: DateTime<Utc>,
    ) -> Result<i64, StorageError>;
    async fn snapshot(
        &self,
        scope_id: &str,
        product_id: Option<&str>,
        metric: &str,
    ) -> Result<Option<CurrentValue>, StorageError>;
    async fn begin_candidate(
        &self,
        context: &WriteContext,
        run_id: &str,
        partition: &Partition,
    ) -> Result<(), StorageError>;
    /// Compare-and-swap the previous cursor, stage the page, and advance together.
    async fn stage_page(
        &self,
        context: &WriteContext,
        run_id: &str,
        previous_cursor: Option<&str>,
        next_cursor: &str,
        facts: &[Fact],
    ) -> Result<(), StorageError>;
    async fn candidate_cursor(&self, run_id: &str) -> Result<Option<String>, StorageError>;
    /// The caller (provider adapter) must attest complete, settled, untruncated authoritative coverage.
    async fn complete_candidate(
        &self,
        context: &WriteContext,
        run_id: &str,
        expected_cursor: Option<&str>,
    ) -> Result<(), StorageError>;
    async fn publish(
        &self,
        context: &WriteContext,
        run_id: &str,
        succeeded_at: DateTime<Utc>,
        watermark: &str,
    ) -> Result<i64, StorageError>;
    async fn published(
        &self,
        partition: &Partition,
    ) -> Result<Option<PublishedPartition>, StorageError>;
    /// Every snapshot of the given scopes, ordered by scope then metric.
    /// An empty scope list returns an empty result without touching the store.
    async fn snapshots(&self, scope_ids: &[String]) -> Result<Vec<CurrentValue>, StorageError>;
    /// Published partitions of the given scopes whose interval intersects
    /// `range`, each with its facts and coverage rows. The read is bounded:
    /// it fails with `StorageError::ResourceLimit` instead of returning a
    /// truncated result once the fact or partition cap is exceeded.
    async fn published_partitions(
        &self,
        scope_ids: &[String],
        range: &Interval,
    ) -> Result<Vec<StoredPartition>, StorageError>;
}
