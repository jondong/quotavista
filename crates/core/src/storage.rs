//! Backend storage ports. No paths, SQL, transport serialization, or credentials.
use crate::{
    calendar::Interval,
    domain::{Lifecycle, Provenance, Value},
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
    async fn set_lifecycle(
        &self,
        context: &WriteContext,
        lifecycle: Lifecycle,
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
}
