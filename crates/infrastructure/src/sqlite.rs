//! Explicit-path SQLite adapter. The desktop composition root does not open it yet.
use chrono::{DateTime, SecondsFormat, Utc};
use quotavista_core::{
    calendar::Interval,
    domain::{CoverageStatus, Lifecycle, Provenance, Reason, Value},
    journal::{OperationKind, OperationPhase, OperationRecord},
    number::{Money, TokenTotal},
    storage::*,
};
use sqlx::{
    migrate::Migrator,
    sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
    },
    Row, Sqlite, SqliteConnection, SqlitePool, Transaction,
};
use std::{path::Path, time::Duration};

pub const SQLITE_VERSION: &str = "3.51.3";
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
pub const READ_CONNECTIONS: u32 = 3;
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Maximum facts a single statistics read may touch; larger stores answer with
/// `StorageError::ResourceLimit` instead of an unbounded scan.
pub const MAX_STATISTICS_FACTS: i64 = 100_000;

#[derive(Clone)]
pub struct SqliteRepository {
    writer: SqlitePool,
    readers: SqlitePool,
}

fn db_error(error: sqlx::Error) -> StorageError {
    if error.as_database_error().is_some_and(|e| {
        e.is_unique_violation() || e.is_foreign_key_violation() || e.is_check_violation()
    }) {
        StorageError::Conflict
    } else {
        StorageError::Unavailable
    }
}

fn timestamp(at: DateTime<Utc>) -> Result<String, StorageError> {
    // A fixed-width UTC representation makes indexed interval comparisons exact.
    if !(0..=9999).contains(&chrono::Datelike::year(&at)) {
        return Err(StorageError::Validation);
    }
    Ok(at.to_rfc3339_opts(SecondsFormat::Nanos, true))
}
fn time(row: &SqliteRow, column: &str) -> Result<DateTime<Utc>, StorageError> {
    text(row, column)?
        .parse()
        .map_err(|_| StorageError::Unavailable)
}
fn text<'a>(row: &'a SqliteRow, column: &str) -> Result<&'a str, StorageError> {
    row.try_get(column).map_err(db_error)
}
fn decode_reason(value: &str) -> Result<Reason, StorageError> {
    match value {
        "permission_required" => Ok(Reason::PermissionRequired),
        "not_yet_settled" => Ok(Reason::NotYetSettled),
        "outside_lookback" => Ok(Reason::OutsideLookback),
        "truncated" => Ok(Reason::Truncated),
        "bucket_boundary_mismatch" => Ok(Reason::BucketBoundaryMismatch),
        "collection_gap" => Ok(Reason::CollectionGap),
        "sync_pending" => Ok(Reason::SyncPending),
        "unsupported" => Ok(Reason::Unsupported),
        "no_data" => Ok(Reason::NoData),
        "incompatible_unit" => Ok(Reason::IncompatibleUnit),
        "unknown_scope" => Ok(Reason::UnknownScope),
        _ => Err(StorageError::Unavailable),
    }
}

fn nonempty(value: &str) -> Result<(), StorageError> {
    if value.is_empty() || value.len() > 4096 {
        Err(StorageError::Validation)
    } else {
        Ok(())
    }
}
fn lifecycle(value: Lifecycle) -> &'static str {
    match value {
        Lifecycle::PendingSetup => "pending_setup",
        Lifecycle::Active => "active",
        Lifecycle::Disabled => "disabled",
        Lifecycle::NeedsCredentials => "needs_credentials",
        Lifecycle::DeletionPending => "deletion_pending",
    }
}
fn decode_lifecycle(value: &str) -> Result<Lifecycle, StorageError> {
    match value {
        "pending_setup" => Ok(Lifecycle::PendingSetup),
        "active" => Ok(Lifecycle::Active),
        "disabled" => Ok(Lifecycle::Disabled),
        "needs_credentials" => Ok(Lifecycle::NeedsCredentials),
        "deletion_pending" => Ok(Lifecycle::DeletionPending),
        _ => Err(StorageError::Unavailable),
    }
}
fn provenance(value: &str) -> Result<Provenance, StorageError> {
    match value {
        "synthetic" => Ok(Provenance::Synthetic),
        "official_event" => Ok(Provenance::OfficialEvent),
        "official_bucket" => Ok(Provenance::OfficialBucket),
        "validated_derivation" => Ok(Provenance::ValidatedDerivation),
        "observation" => Ok(Provenance::Observation),
        _ => Err(StorageError::Unavailable),
    }
}
fn encode_value(value: &Value) -> Result<(&'static str, String, String), StorageError> {
    Ok(match value {
        Value::Tokens(n) => ("tokens", n.0.to_string(), String::new()),
        Value::Money(m) => ("money", m.amount.to_string(), m.currency.to_string()),
        Value::Ratio(n) => ("ratio", n.to_string(), String::new()),
        Value::PercentagePoints(n) => ("percentage_points", n.to_string(), String::new()),
        Value::Native { amount, unit } => {
            nonempty(unit)?;
            ("native", amount.to_string(), unit.clone())
        }
    })
}
fn decode_value(row: &SqliteRow) -> Result<Value, StorageError> {
    let amount = text(row, "amount")?;
    let detail = text(row, "unit_detail")?;
    let value = match text(row, "value_kind")? {
        "tokens" if detail.is_empty() => Value::Tokens(TokenTotal(
            amount.parse().map_err(|_| StorageError::Unavailable)?,
        )),
        "money" => Value::Money(Money {
            amount: amount.parse().map_err(|_| StorageError::Unavailable)?,
            currency: detail.parse().map_err(|_| StorageError::Unavailable)?,
        }),
        "ratio" if detail.is_empty() => {
            Value::Ratio(amount.parse().map_err(|_| StorageError::Unavailable)?)
        }
        "percentage_points" if detail.is_empty() => {
            Value::PercentagePoints(amount.parse().map_err(|_| StorageError::Unavailable)?)
        }
        "native" if !detail.is_empty() => Value::Native {
            amount: amount.parse().map_err(|_| StorageError::Unavailable)?,
            unit: detail.into(),
        },
        _ => return Err(StorageError::Unavailable),
    };
    if encode_value(&value)?.1 != amount {
        return Err(StorageError::Unavailable);
    }
    Ok(value)
}

impl SqliteRepository {
    /// Caller supplies a local path and single-instance/recovery coordination.
    /// Tests pass only paths owned by tempfile::TempDir, never an application directory.
    pub async fn open(path: &Path) -> Result<Self, StorageError> {
        Self::open_with_migrator(path, &MIGRATOR).await
    }

    async fn open_with_migrator(path: &Path, migrations: &Migrator) -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .pragma("wal_autocheckpoint", "1000");
        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(BUSY_TIMEOUT)
            .connect_with(options.clone())
            .await
            .map_err(db_error)?;
        let setup = async {
            let version: String = sqlx::query_scalar("SELECT sqlite_version()")
                .fetch_one(&writer).await.map_err(db_error)?;
            if version != SQLITE_VERSION { return Err(StorageError::IncompatibleVersion); }
            // There is no released predecessor schema. Refuse populated upgrades until
            // the maintenance/consistent-backup coordinator is installed (not a file copy).
            let tables: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'")
                .fetch_one(&writer).await.map_err(db_error)?;
            if tables > 0 {
                let latest: i64 = sqlx::query_scalar("SELECT coalesce(max(version), 0) FROM _sqlx_migrations WHERE success=1")
                    .fetch_one(&writer).await.map_err(|_| StorageError::Migration)?;
                if migrations.iter().any(|m| m.version > latest) { return Err(StorageError::Migration); }
            }
            migrations.run(&writer).await.map_err(|_| StorageError::Migration)
        }.await;
        if let Err(error) = setup {
            writer.close().await;
            return Err(error);
        }
        let readers = match SqlitePoolOptions::new()
            .max_connections(READ_CONNECTIONS)
            .acquire_timeout(BUSY_TIMEOUT)
            .connect_with(options.create_if_missing(false).read_only(true))
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                writer.close().await;
                return Err(db_error(error));
            }
        };
        Ok(Self { writer, readers })
    }

    pub async fn close(&self) {
        self.readers.close().await;
        self.writer.close().await;
    }

    async fn write(&self) -> Result<Transaction<'_, Sqlite>, StorageError> {
        self.writer
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db_error)
    }
}

/// Mutation fence, checked inside the write transaction: the connection must
/// still exist at the captured generation with an active lifecycle, the scope
/// must belong to it, and no incomplete lifecycle operation may be pending for
/// it. `operation` excludes one operation from the fence so the operation that
/// is committing its own activation can proceed.
async fn guard(
    conn: &mut SqliteConnection,
    context: &WriteContext,
    scope: Option<&str>,
    operation: Option<&str>,
) -> Result<(), StorageError> {
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM connections c WHERE c.id=? AND c.generation=? AND c.lifecycle='active' AND (? IS NULL OR EXISTS(SELECT 1 FROM provider_scopes s WHERE s.id=? AND s.connection_id=c.id)) AND NOT EXISTS(SELECT 1 FROM lifecycle_operations o WHERE o.connection_id=c.id AND o.state <> 'complete' AND (? IS NULL OR o.id <> ?)))")
        .bind(&context.connection_id).bind(context.generation).bind(scope).bind(scope).bind(operation).bind(operation)
        .fetch_one(conn).await.map_err(db_error)?;
    if valid {
        Ok(())
    } else {
        Err(StorageError::Conflict)
    }
}

/// Lifecycle-mutation fence: the connection must exist at the captured
/// generation and no *other* incomplete operation may be pending for it. Unlike
/// [`guard`] it does not require an active lifecycle, because activating a
/// candidate reference is what moves a pending setup to active.
async fn mutation_fence(
    conn: &mut SqliteConnection,
    context: &WriteContext,
    operation: &str,
) -> Result<(), StorageError> {
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM connections c WHERE c.id=? AND c.generation=? AND c.lifecycle <> 'deletion_pending' AND NOT EXISTS(SELECT 1 FROM lifecycle_operations o WHERE o.connection_id=c.id AND o.state <> 'complete' AND o.id <> ?))")
        .bind(&context.connection_id).bind(context.generation).bind(operation)
        .fetch_one(conn).await.map_err(db_error)?;
    if valid {
        Ok(())
    } else {
        Err(StorageError::Conflict)
    }
}

fn operation_record(row: &SqliteRow) -> Result<OperationRecord, StorageError> {
    let record = OperationRecord {
        operation_id: text(row, "id")?.into(),
        connection_id: text(row, "connection_id")?.into(),
        kind: OperationKind::parse(text(row, "kind")?).ok_or(StorageError::Unavailable)?,
        phase: OperationPhase::parse(text(row, "state")?).ok_or(StorageError::Unavailable)?,
        candidate_ref: row
            .try_get::<Option<String>, _>("candidate_ref")
            .map_err(db_error)?,
        previous_ref: row
            .try_get::<Option<String>, _>("previous_ref")
            .map_err(db_error)?,
        generation: row.try_get("generation").map_err(db_error)?,
    };
    record.validate().map_err(|_| StorageError::Unavailable)?;
    Ok(record)
}

async fn revision(conn: &mut SqliteConnection) -> Result<i64, StorageError> {
    sqlx::query_scalar(
        "UPDATE store_state SET data_revision=data_revision+1 WHERE id=1 RETURNING data_revision",
    )
    .fetch_one(conn)
    .await
    .map_err(db_error)
}
async fn partition_id(
    conn: &mut SqliteConnection,
    p: &Partition,
) -> Result<Option<i64>, StorageError> {
    sqlx::query_scalar("SELECT id FROM partitions WHERE scope_id=? AND product_key=? AND dataset=? AND metric=? AND start_at=? AND end_at=? AND grain=? AND dimensions=? AND authority=?")
        .bind(&p.scope_id).bind(&p.product_key).bind(&p.dataset).bind(&p.metric)
        .bind(timestamp(p.interval.start)?).bind(timestamp(p.interval.end)?)
        .bind(&p.grain).bind(&p.dimensions).bind(p.authority.as_str())
        .fetch_optional(conn).await.map_err(db_error)
}
async fn run(
    conn: &mut SqliteConnection,
    context: &WriteContext,
    id: &str,
) -> Result<SqliteRow, StorageError> {
    let row = sqlx::query("SELECT r.*, p.scope_id,p.product_key,p.dataset,p.metric,p.start_at,p.end_at,p.grain,p.dimensions,p.authority FROM sync_runs r JOIN partitions p ON p.id=r.partition_id WHERE r.id=? AND r.generation=?")
        .bind(id).bind(context.generation).fetch_optional(&mut *conn).await.map_err(db_error)?.ok_or(StorageError::Conflict)?;
    guard(conn, context, Some(text(&row, "scope_id")?), None).await?;
    Ok(row)
}
fn fact(row: SqliteRow) -> Result<Fact, StorageError> {
    let value = decode_value(&row)?;
    if matches!(value, Value::Tokens(TokenTotal(n)) if n > u64::MAX.into()) {
        return Err(StorageError::Unavailable);
    }
    Ok(Fact {
        identity: match text(&row, "kind")? {
            "event" => FactIdentity::Event(text(&row, "event_id")?.into()),
            "bucket" => FactIdentity::Bucket,
            _ => return Err(StorageError::Unavailable),
        },
        interval: Interval::new(time(&row, "start_at")?, time(&row, "end_at")?)
            .map_err(|_| StorageError::Unavailable)?,
        value,
        source_revision: text(&row, "source_revision")?.into(),
    })
}

impl Repository for SqliteRepository {
    async fn settings(&self) -> Result<Settings, StorageError> {
        let row = sqlx::query("SELECT * FROM settings WHERE id=1")
            .fetch_one(&self.readers)
            .await
            .map_err(db_error)?;
        Ok(Settings {
            revision: row.try_get("revision").map_err(db_error)?,
            locale: text(&row, "locale")?.into(),
            reporting_zone: text(&row, "reporting_zone")?
                .parse()
                .map_err(|_| StorageError::Unavailable)?,
            currency: text(&row, "currency")?
                .parse()
                .map_err(|_| StorageError::Unavailable)?,
            refresh_minutes: row
                .try_get::<Option<i64>, _>("refresh_minutes")
                .map_err(db_error)?
                .map(|n| u8::try_from(n).map_err(|_| StorageError::Unavailable))
                .transpose()?,
            quota_threshold: text(&row, "quota_threshold")?
                .parse()
                .map_err(|_| StorageError::Unavailable)?,
            balance_threshold: text(&row, "balance_threshold")?
                .parse()
                .map_err(|_| StorageError::Unavailable)?,
        })
    }
    async fn save_settings(&self, settings: &Settings) -> Result<Settings, StorageError> {
        if !matches!(settings.locale.as_str(), "en" | "zh-CN")
            || !matches!(settings.refresh_minutes, None | Some(1 | 5 | 15))
        {
            return Err(StorageError::Validation);
        }
        let mut tx = self.write().await?;
        let next: Option<i64> = sqlx::query_scalar("UPDATE settings SET revision=revision+1,locale=?,reporting_zone=?,currency=?,refresh_minutes=?,quota_threshold=?,balance_threshold=? WHERE id=1 AND revision=? RETURNING revision")
            .bind(&settings.locale).bind(settings.reporting_zone.name()).bind(settings.currency.to_string()).bind(settings.refresh_minutes.map(i64::from)).bind(settings.quota_threshold.to_string()).bind(settings.balance_threshold.to_string()).bind(settings.revision)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let next = next.ok_or(StorageError::Conflict)?;
        tx.commit().await.map_err(db_error)?;
        Ok(Settings {
            revision: next,
            ..settings.clone()
        })
    }
    async fn create_connection(&self, c: &Connection, s: &Scope) -> Result<(), StorageError> {
        for key in [&c.id, &c.provider, &s.id, &s.key, &s.kind] {
            nonempty(key)?;
        }
        if c.generation < 0
            || s.connection_id != c.id
            || c.alias.trim() != c.alias
            || c.alias.chars().count() > 80
            || c.alias.chars().any(char::is_control)
            || c.alias.is_empty()
        {
            return Err(StorageError::Validation);
        }
        let mut tx = self.write().await?;
        sqlx::query(
            "INSERT INTO connections(id,provider,alias,generation,lifecycle) VALUES(?,?,?,?,?)",
        )
        .bind(&c.id)
        .bind(&c.provider)
        .bind(&c.alias)
        .bind(c.generation)
        .bind(lifecycle(c.lifecycle))
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "INSERT INTO provider_scopes(id,connection_id,scope_key,scope_type) VALUES(?,?,?,?)",
        )
        .bind(&s.id)
        .bind(&s.connection_id)
        .bind(&s.key)
        .bind(&s.kind)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)
    }
    async fn connection(&self, id: &str) -> Result<Option<Connection>, StorageError> {
        sqlx::query("SELECT * FROM connections WHERE id=?")
            .bind(id)
            .fetch_optional(&self.readers)
            .await
            .map_err(db_error)?
            .map(|r| {
                Ok(Connection {
                    id: text(&r, "id")?.into(),
                    provider: text(&r, "provider")?.into(),
                    alias: text(&r, "alias")?.into(),
                    generation: r.try_get("generation").map_err(db_error)?,
                    lifecycle: decode_lifecycle(text(&r, "lifecycle")?)?,
                })
            })
            .transpose()
    }
    async fn connections(&self) -> Result<Vec<Connection>, StorageError> {
        let rows = sqlx::query("SELECT * FROM connections ORDER BY id")
            .fetch_all(&self.readers)
            .await
            .map_err(db_error)?;
        rows.into_iter()
            .map(|r| {
                Ok(Connection {
                    id: text(&r, "id")?.into(),
                    provider: text(&r, "provider")?.into(),
                    alias: text(&r, "alias")?.into(),
                    generation: r.try_get("generation").map_err(db_error)?,
                    lifecycle: decode_lifecycle(text(&r, "lifecycle")?)?,
                })
            })
            .collect()
    }
    async fn data_revision(&self) -> Result<i64, StorageError> {
        sqlx::query_scalar("SELECT data_revision FROM store_state WHERE id=1")
            .fetch_one(&self.readers)
            .await
            .map_err(db_error)
    }
    async fn set_lifecycle(
        &self,
        context: &WriteContext,
        state: Lifecycle,
    ) -> Result<i64, StorageError> {
        let mut tx = self.write().await?;
        // The fence is re-checked inside the write transaction: a lifecycle
        // mutation is itself a lifecycle event, so an incomplete operation row must
        // refuse it even when the captured generation still matches.
        let next: Option<i64> = sqlx::query_scalar("UPDATE connections SET lifecycle=?,generation=generation+1 WHERE id=? AND generation=? AND NOT EXISTS(SELECT 1 FROM lifecycle_operations o WHERE o.connection_id=connections.id AND o.state <> 'complete') RETURNING generation")
            .bind(lifecycle(state)).bind(&context.connection_id).bind(context.generation)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let next = next.ok_or(StorageError::Conflict)?;
        tx.commit().await.map_err(db_error)?;
        Ok(next)
    }
    async fn set_alias(&self, context: &WriteContext, alias: &str) -> Result<i64, StorageError> {
        // Same alias rules as create_connection: trimmed, nonempty, bounded,
        // and free of control characters.
        if alias.trim() != alias
            || alias.chars().count() > 80
            || alias.chars().any(char::is_control)
            || alias.is_empty()
        {
            return Err(StorageError::Validation);
        }
        let mut tx = self.write().await?;
        // The write re-checks the captured generation, refuses a connection
        // pending deletion, and observes the same operation fence as the other
        // lifecycle mutations.
        let next: Option<i64> = sqlx::query_scalar("UPDATE connections SET alias=?,generation=generation+1 WHERE id=? AND generation=? AND lifecycle <> 'deletion_pending' AND NOT EXISTS(SELECT 1 FROM lifecycle_operations o WHERE o.connection_id=connections.id AND o.state <> 'complete') RETURNING generation")
            .bind(alias).bind(&context.connection_id).bind(context.generation)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let next = next.ok_or(StorageError::Conflict)?;
        tx.commit().await.map_err(db_error)?;
        Ok(next)
    }
    async fn delete_connection(&self, connection_id: &str) -> Result<(), StorageError> {
        nonempty(connection_id)?;
        let mut tx = self.write().await?;
        // Cascades take scopes, projections, and usage data with the row; the
        // opaque managed-backup membership deliberately survives until backup
        // cleanup exists. An absent row is already deleted, so it is a success.
        sqlx::query("DELETE FROM connections WHERE id=?")
            .bind(connection_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(())
    }
    async fn scopes(&self, connection_id: &str) -> Result<Vec<Scope>, StorageError> {
        nonempty(connection_id)?;
        let rows = sqlx::query("SELECT * FROM provider_scopes WHERE connection_id=? ORDER BY id")
            .bind(connection_id)
            .fetch_all(&self.readers)
            .await
            .map_err(db_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(Scope {
                    id: text(&row, "id")?.to_owned(),
                    connection_id: text(&row, "connection_id")?.to_owned(),
                    key: text(&row, "scope_key")?.to_owned(),
                    kind: text(&row, "scope_type")?.to_owned(),
                })
            })
            .collect()
    }
    async fn advance_data_revision(&self) -> Result<i64, StorageError> {
        let mut tx = self.write().await?;
        let next = revision(&mut tx).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(next)
    }
    async fn scope_ids(&self, connection_id: &str) -> Result<Vec<String>, StorageError> {
        nonempty(connection_id)?;
        sqlx::query_scalar("SELECT id FROM provider_scopes WHERE connection_id=? ORDER BY id")
            .bind(connection_id)
            .fetch_all(&self.readers)
            .await
            .map_err(db_error)
    }
    async fn credential_ref(&self, connection_id: &str) -> Result<Option<String>, StorageError> {
        nonempty(connection_id)?;
        let row: Option<Option<String>> =
            sqlx::query_scalar("SELECT credential_ref FROM connections WHERE id=?")
                .bind(connection_id)
                .fetch_optional(&self.readers)
                .await
                .map_err(db_error)?;
        // A missing connection is a conflict; a present one may hold no reference.
        row.ok_or(StorageError::Conflict)
    }
    async fn begin_operation(&self, record: &OperationRecord) -> Result<(), StorageError> {
        record.validate().map_err(|_| StorageError::Validation)?;
        let mut tx = self.write().await?;
        let rows = sqlx::query("INSERT INTO lifecycle_operations(id,connection_id,kind,state,candidate_ref,previous_ref,generation) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET connection_id=excluded.connection_id,kind=excluded.kind,state=excluded.state,candidate_ref=excluded.candidate_ref,previous_ref=excluded.previous_ref,generation=excluded.generation WHERE lifecycle_operations.state <> 'complete'")
            .bind(&record.operation_id).bind(&record.connection_id).bind(record.kind.as_str())
            .bind(record.phase.as_str()).bind(record.candidate_ref.as_deref())
            .bind(record.previous_ref.as_deref()).bind(record.generation)
            .execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        if rows == 0 {
            // A completed projection is final: the operation may not be reopened.
            return Err(StorageError::Conflict);
        }
        tx.commit().await.map_err(db_error)
    }
    async fn advance_operation(&self, record: &OperationRecord) -> Result<(), StorageError> {
        record.validate().map_err(|_| StorageError::Validation)?;
        let mut tx = self.write().await?;
        let rows = sqlx::query("UPDATE lifecycle_operations SET state=?,candidate_ref=?,previous_ref=?,generation=? WHERE id=? AND state <> 'complete'")
            .bind(record.phase.as_str()).bind(record.candidate_ref.as_deref())
            .bind(record.previous_ref.as_deref()).bind(record.generation).bind(&record.operation_id)
            .execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        if rows == 0 {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM lifecycle_operations WHERE id=?)")
                    .bind(&record.operation_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db_error)?;
            // A projection that was never written holds no fence, so recovery may
            // complete it; a completed projection may not be reopened.
            if exists || record.phase != OperationPhase::Complete {
                return Err(StorageError::Conflict);
            }
        }
        tx.commit().await.map_err(db_error)
    }
    async fn operations(
        &self,
        connection_id: Option<&str>,
    ) -> Result<Vec<OperationRecord>, StorageError> {
        if let Some(id) = connection_id {
            nonempty(id)?;
        }
        let rows = sqlx::query(
            "SELECT * FROM lifecycle_operations WHERE state <> 'complete' AND (? IS NULL OR connection_id=?) ORDER BY id",
        )
        .bind(connection_id)
        .bind(connection_id)
        .fetch_all(&self.readers)
        .await
        .map_err(db_error)?;
        rows.iter().map(operation_record).collect()
    }
    async fn activate_reference(
        &self,
        context: &WriteContext,
        operation_id: &str,
        candidate_ref: &str,
        previous_ref: Option<&str>,
    ) -> Result<i64, StorageError> {
        nonempty(operation_id)?;
        nonempty(candidate_ref)?;
        let mut tx = self.write().await?;
        mutation_fence(&mut tx, context, operation_id).await?;
        let rows = sqlx::query("UPDATE lifecycle_operations SET state='committed',candidate_ref=?,previous_ref=coalesce(?,previous_ref),generation=? WHERE id=? AND connection_id=? AND state <> 'complete'")
            .bind(candidate_ref).bind(previous_ref).bind(context.generation)
            .bind(operation_id).bind(&context.connection_id)
            .execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        if rows != 1 {
            return Err(StorageError::Conflict);
        }
        // Credential replacement increments the generation, so a task that
        // captured the previous generation can no longer publish results.
        let next: Option<i64> = sqlx::query_scalar("UPDATE connections SET credential_ref=?,generation=generation+1,lifecycle=CASE WHEN lifecycle IN ('pending_setup','needs_credentials') THEN 'active' ELSE lifecycle END WHERE id=? AND generation=? RETURNING generation")
            .bind(candidate_ref).bind(&context.connection_id).bind(context.generation)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let next = next.ok_or(StorageError::Conflict)?;
        tx.commit().await.map_err(db_error)?;
        Ok(next)
    }
    async fn save_snapshot(
        &self,
        context: &WriteContext,
        snapshot: &Snapshot,
        succeeded_at: DateTime<Utc>,
    ) -> Result<i64, StorageError> {
        nonempty(&snapshot.metric)?;
        if snapshot.product_id.as_deref() == Some("") {
            return Err(StorageError::Validation);
        }
        let (kind, amount, detail) = encode_value(&snapshot.value)?;
        let mut tx = self.write().await?;
        guard(&mut tx, context, Some(&snapshot.scope_id), None).await?;
        let next = revision(&mut tx).await?;
        sqlx::query("INSERT INTO snapshots(scope_id,product_key,product_id,metric,value_kind,amount,unit_detail,provenance,observed_at,source_as_of,successful_sync_at,data_revision) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(scope_id,product_key,metric) DO UPDATE SET value_kind=excluded.value_kind,amount=excluded.amount,unit_detail=excluded.unit_detail,provenance=excluded.provenance,observed_at=excluded.observed_at,source_as_of=excluded.source_as_of,successful_sync_at=excluded.successful_sync_at,data_revision=excluded.data_revision")
            .bind(&snapshot.scope_id).bind(snapshot.product_id.as_deref().unwrap_or("")).bind(&snapshot.product_id).bind(&snapshot.metric)
            .bind(kind).bind(amount).bind(detail).bind(snapshot.provenance.as_str()).bind(timestamp(snapshot.observed_at)?)
            .bind(snapshot.source_as_of.map(timestamp).transpose()?).bind(timestamp(succeeded_at)?).bind(next)
            .execute(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(next)
    }
    async fn snapshot(
        &self,
        scope_id: &str,
        product_id: Option<&str>,
        metric: &str,
    ) -> Result<Option<CurrentValue>, StorageError> {
        sqlx::query("SELECT * FROM snapshots WHERE scope_id=? AND product_key=? AND metric=?")
            .bind(scope_id)
            .bind(product_id.unwrap_or(""))
            .bind(metric)
            .fetch_optional(&self.readers)
            .await
            .map_err(db_error)?
            .map(|r| {
                Ok(CurrentValue {
                    snapshot: Snapshot {
                        scope_id: scope_id.into(),
                        product_id: r.try_get("product_id").map_err(db_error)?,
                        metric: metric.into(),
                        value: decode_value(&r)?,
                        provenance: provenance(text(&r, "provenance")?)?,
                        observed_at: time(&r, "observed_at")?,
                        source_as_of: r
                            .try_get::<Option<String>, _>("source_as_of")
                            .map_err(db_error)?
                            .map(|s| s.parse().map_err(|_| StorageError::Unavailable))
                            .transpose()?,
                    },
                    successful_sync_at: time(&r, "successful_sync_at")?,
                    data_revision: r.try_get("data_revision").map_err(db_error)?,
                })
            })
            .transpose()
    }

    async fn snapshots(&self, scope_ids: &[String]) -> Result<Vec<CurrentValue>, StorageError> {
        // Static SQL per scope: the caller caps scope counts, and dynamic SQL
        // fails the sqlx safe-string audit.
        let mut out = Vec::new();
        for scope_id in scope_ids {
            let rows =
                sqlx::query("SELECT * FROM snapshots WHERE scope_id=? ORDER BY product_key,metric")
                    .bind(scope_id)
                    .fetch_all(&self.readers)
                    .await
                    .map_err(db_error)?;
            for r in &rows {
                out.push(CurrentValue {
                    snapshot: Snapshot {
                        scope_id: text(r, "scope_id")?.into(),
                        product_id: r.try_get("product_id").map_err(db_error)?,
                        metric: text(r, "metric")?.into(),
                        value: decode_value(r)?,
                        provenance: provenance(text(r, "provenance")?)?,
                        observed_at: time(r, "observed_at")?,
                        source_as_of: r
                            .try_get::<Option<String>, _>("source_as_of")
                            .map_err(db_error)?
                            .map(|s| s.parse().map_err(|_| StorageError::Unavailable))
                            .transpose()?,
                    },
                    successful_sync_at: time(r, "successful_sync_at")?,
                    data_revision: r.try_get("data_revision").map_err(db_error)?,
                });
            }
        }
        Ok(out)
    }

    async fn published_partitions(
        &self,
        scope_ids: &[String],
        range: &Interval,
    ) -> Result<Vec<StoredPartition>, StorageError> {
        if scope_ids.is_empty() {
            return Ok(Vec::new());
        }
        // Header, facts, and coverage must come from one read snapshot so a
        // replacement publish can never split the view.
        let mut tx = self.readers.begin().await.map_err(db_error)?;
        let mut total: i64 = 0;
        for scope_id in scope_ids {
            total += sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_facts u JOIN partitions p ON p.id=u.partition_id WHERE p.scope_id=? AND p.start_at<? AND p.end_at>?")
                .bind(scope_id)
                .bind(timestamp(range.end)?)
                .bind(timestamp(range.start)?)
                .fetch_one(&mut *tx)
                .await
                .map_err(db_error)?;
        }
        if total > MAX_STATISTICS_FACTS {
            return Err(StorageError::ResourceLimit);
        }
        let mut out = Vec::new();
        for scope_id in scope_ids {
            let headers = sqlx::query("SELECT p.id,p.scope_id,p.product_key,p.dataset,p.metric,p.start_at,p.end_at,p.grain,p.dimensions,p.authority,pp.data_revision,pp.successful_sync_at,pp.watermark FROM published_partitions pp JOIN partitions p ON p.id=pp.partition_id WHERE p.scope_id=? AND p.start_at<? AND p.end_at>? ORDER BY p.dataset,p.metric,p.start_at")
                .bind(scope_id)
                .bind(timestamp(range.end)?)
                .bind(timestamp(range.start)?)
                .fetch_all(&mut *tx)
                .await
                .map_err(db_error)?;
            for row in headers {
                let id: i64 = row.try_get("id").map_err(db_error)?;
                let partition = Partition {
                    scope_id: text(&row, "scope_id")?.into(),
                    product_key: text(&row, "product_key")?.into(),
                    dataset: text(&row, "dataset")?.into(),
                    metric: text(&row, "metric")?.into(),
                    interval: Interval::new(time(&row, "start_at")?, time(&row, "end_at")?)
                        .map_err(|_| StorageError::Unavailable)?,
                    grain: text(&row, "grain")?.into(),
                    dimensions: text(&row, "dimensions")?.into(),
                    authority: provenance(text(&row, "authority")?)?,
                };
                let facts = sqlx::query("SELECT * FROM usage_facts WHERE partition_id=? ORDER BY kind,event_id,key_start,key_end")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(db_error)?
                    .into_iter()
                    .map(fact)
                    .collect::<Result<_, _>>()?;
                let coverage = sqlx::query("SELECT * FROM coverage_intervals WHERE partition_id=?")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(db_error)?
                    .iter()
                    .map(|r| {
                        Ok(CoverageRow {
                            interval: partition.interval,
                            status: match text(r, "status")? {
                                "complete" => CoverageStatus::Complete,
                                "partial" => CoverageStatus::Partial,
                                "missing" => CoverageStatus::Missing,
                                "unsupported" => CoverageStatus::Unsupported,
                                _ => return Err(StorageError::Unavailable),
                            },
                            reason: r
                                .try_get::<Option<String>, _>("reason")
                                .map_err(db_error)?
                                .map(|s| decode_reason(&s))
                                .transpose()?,
                            zone: text(r, "source_zone")?.into(),
                        })
                    })
                    .collect::<Result<_, StorageError>>()?;
                out.push(StoredPartition {
                    partition,
                    facts,
                    coverage,
                    data_revision: row.try_get("data_revision").map_err(db_error)?,
                    successful_sync_at: time(&row, "successful_sync_at")?,
                    watermark: text(&row, "watermark")?.into(),
                });
            }
        }
        Ok(out)
    }
    async fn begin_candidate(
        &self,
        context: &WriteContext,
        run_id: &str,
        p: &Partition,
    ) -> Result<(), StorageError> {
        for key in [run_id, &p.scope_id, &p.dataset, &p.metric, &p.grain] {
            nonempty(key)?;
        }
        if !matches!(
            p.authority,
            Provenance::Synthetic | Provenance::OfficialEvent | Provenance::OfficialBucket
        ) || p.interval.start >= p.interval.end
        {
            return Err(StorageError::Validation);
        }
        let mut tx = self.write().await?;
        guard(&mut tx, context, Some(&p.scope_id), None).await?;
        if !p.product_key.is_empty() {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM products WHERE id=? AND scope_id=?)",
            )
            .bind(&p.product_key)
            .bind(&p.scope_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_error)?;
            if !exists {
                return Err(StorageError::Conflict);
            }
        }
        sqlx::query("INSERT INTO partitions(scope_id,product_key,dataset,metric,start_at,end_at,grain,dimensions,authority) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(&p.scope_id).bind(&p.product_key).bind(&p.dataset).bind(&p.metric).bind(timestamp(p.interval.start)?).bind(timestamp(p.interval.end)?).bind(&p.grain).bind(&p.dimensions).bind(p.authority.as_str())
            .execute(&mut *tx).await.map_err(db_error)?;
        let id = partition_id(&mut tx, p)
            .await?
            .ok_or(StorageError::Unavailable)?;
        sqlx::query("INSERT INTO sync_runs(id,partition_id,generation,base_revision,state) VALUES(?,?,?,coalesce((SELECT data_revision FROM published_partitions WHERE partition_id=?),0),'incomplete')")
            .bind(run_id).bind(id).bind(context.generation).bind(id).execute(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)
    }
    async fn stage_page(
        &self,
        context: &WriteContext,
        run_id: &str,
        previous_cursor: Option<&str>,
        next_cursor: &str,
        facts: &[Fact],
    ) -> Result<(), StorageError> {
        nonempty(next_cursor)?;
        if previous_cursor == Some(next_cursor) || facts.len() > 1000 {
            return Err(StorageError::Validation);
        }
        let mut tx = self.write().await?;
        let r = run(&mut tx, context, run_id).await?;
        if text(&r, "state")? != "incomplete"
            || r.try_get::<Option<&str>, _>("cursor").map_err(db_error)? != previous_cursor
        {
            return Err(StorageError::Conflict);
        }
        let start = time(&r, "start_at")?;
        let end = time(&r, "end_at")?;
        for f in facts {
            if f.interval.start < start
                || f.interval.end > end
                || f.interval.start >= f.interval.end
                || matches!(f.value, Value::Tokens(TokenTotal(n)) if n > u64::MAX.into())
            {
                return Err(StorageError::Validation);
            }
            let (kind, event_id, key_start, key_end) = match &f.identity {
                FactIdentity::Event(id) => {
                    nonempty(id)?;
                    if text(&r, "authority")? == "official_bucket" {
                        return Err(StorageError::Validation);
                    }
                    ("event", id.as_str(), String::new(), String::new())
                }
                FactIdentity::Bucket => {
                    if text(&r, "authority")? == "official_event" {
                        return Err(StorageError::Validation);
                    }
                    (
                        "bucket",
                        "",
                        timestamp(f.interval.start)?,
                        timestamp(f.interval.end)?,
                    )
                }
            };
            let (value_kind, amount, detail) = encode_value(&f.value)?;
            sqlx::query("INSERT INTO staged_facts(run_id,kind,event_id,key_start,key_end,start_at,end_at,value_kind,amount,unit_detail,source_revision) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(run_id,kind,event_id,key_start,key_end) DO UPDATE SET start_at=excluded.start_at,end_at=excluded.end_at,value_kind=excluded.value_kind,amount=excluded.amount,unit_detail=excluded.unit_detail,source_revision=excluded.source_revision")
                .bind(run_id).bind(kind).bind(event_id).bind(key_start).bind(key_end).bind(timestamp(f.interval.start)?).bind(timestamp(f.interval.end)?)
                .bind(value_kind).bind(amount).bind(detail).bind(&f.source_revision).execute(&mut *tx).await.map_err(db_error)?;
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM staged_facts WHERE run_id=?")
            .bind(run_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_error)?;
        if count > 100_000 {
            return Err(StorageError::Validation);
        }
        sqlx::query("UPDATE sync_runs SET cursor=? WHERE id=?")
            .bind(next_cursor)
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)
    }
    async fn candidate_cursor(&self, run_id: &str) -> Result<Option<String>, StorageError> {
        sqlx::query_scalar("SELECT cursor FROM sync_runs WHERE id=?")
            .bind(run_id)
            .fetch_optional(&self.readers)
            .await
            .map_err(db_error)?
            .ok_or(StorageError::Conflict)
    }
    async fn complete_candidate(
        &self,
        context: &WriteContext,
        run_id: &str,
        expected_cursor: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut tx = self.write().await?;
        let r = run(&mut tx, context, run_id).await?;
        if text(&r, "state")? != "incomplete"
            || r.try_get::<Option<&str>, _>("cursor").map_err(db_error)? != expected_cursor
        {
            return Err(StorageError::Conflict);
        }
        sqlx::query("UPDATE sync_runs SET state='complete' WHERE id=?")
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)
    }
    async fn publish(
        &self,
        context: &WriteContext,
        run_id: &str,
        succeeded_at: DateTime<Utc>,
        watermark: &str,
    ) -> Result<i64, StorageError> {
        let mut tx = self.write().await?;
        let r = run(&mut tx, context, run_id).await?;
        let id: i64 = r.try_get("partition_id").map_err(db_error)?;
        if text(&r, "state")? == "published" {
            return r.try_get("published_revision").map_err(db_error);
        }
        if text(&r, "state")? != "complete" {
            return Err(StorageError::Conflict);
        }
        let current: i64 = sqlx::query_scalar("SELECT coalesce((SELECT data_revision FROM published_partitions WHERE partition_id=?),0)").bind(id).fetch_one(&mut *tx).await.map_err(db_error)?;
        if current != r.try_get::<i64, _>("base_revision").map_err(db_error)? {
            return Err(StorageError::Conflict);
        }
        // Never add overlapping event/bucket authorities (including different grains).
        let overlap: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM published_partitions pp JOIN partitions p ON p.id=pp.partition_id WHERE p.id<>? AND p.scope_id=? AND p.product_key=? AND p.dataset=? AND p.metric=? AND p.dimensions=? AND p.start_at<? AND p.end_at>?)")
            .bind(id).bind(text(&r, "scope_id")?).bind(text(&r, "product_key")?).bind(text(&r, "dataset")?).bind(text(&r, "metric")?).bind(text(&r, "dimensions")?).bind(text(&r, "end_at")?).bind(text(&r, "start_at")?)
            .fetch_one(&mut *tx).await.map_err(db_error)?;
        if overlap {
            return Err(StorageError::Conflict);
        }
        sqlx::query("DELETE FROM usage_facts WHERE partition_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        // A cross-partition event correction hits event_identity and rolls back, never duplicates.
        sqlx::query("INSERT INTO usage_facts SELECT ?,?,?,?,kind,event_id,key_start,key_end,start_at,end_at,value_kind,amount,unit_detail,source_revision FROM staged_facts WHERE run_id=?")
            .bind(id).bind(text(&r, "scope_id")?).bind(text(&r, "dataset")?).bind(text(&r, "metric")?).bind(run_id).execute(&mut *tx).await.map_err(db_error)?;
        let next = revision(&mut tx).await?;
        sqlx::query("INSERT INTO published_partitions VALUES(?,?,?,?,?) ON CONFLICT(partition_id) DO UPDATE SET run_id=excluded.run_id,data_revision=excluded.data_revision,successful_sync_at=excluded.successful_sync_at,watermark=excluded.watermark")
            .bind(id).bind(run_id).bind(next).bind(timestamp(succeeded_at)?).bind(watermark).execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query("INSERT INTO coverage_intervals VALUES(?,'UTC','complete',NULL,?) ON CONFLICT(partition_id) DO UPDATE SET status='complete',reason=NULL,data_revision=excluded.data_revision")
            .bind(id).bind(next).execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query("INSERT INTO sync_checkpoints VALUES(?,?,?,?) ON CONFLICT(partition_id) DO UPDATE SET run_id=excluded.run_id,watermark=excluded.watermark,data_revision=excluded.data_revision")
            .bind(id).bind(run_id).bind(watermark).bind(next).execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query("UPDATE sync_runs SET state='published',published_revision=? WHERE id=?")
            .bind(next)
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("DELETE FROM staged_facts WHERE run_id=?")
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(next)
    }
    async fn published(
        &self,
        partition: &Partition,
    ) -> Result<Option<PublishedPartition>, StorageError> {
        // Header and facts must be from the same read snapshot during replacement.
        let mut tx = self.readers.begin().await.map_err(db_error)?;
        let Some(id) = partition_id(&mut tx, partition).await? else {
            return Ok(None);
        };
        let Some(row) = sqlx::query("SELECT * FROM published_partitions WHERE partition_id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?
        else {
            return Ok(None);
        };
        let facts = sqlx::query("SELECT * FROM usage_facts WHERE partition_id=? ORDER BY kind,event_id,key_start,key_end").bind(id).fetch_all(&mut *tx).await.map_err(db_error)?.into_iter().map(fact).collect::<Result<_, _>>()?;
        Ok(Some(PublishedPartition {
            facts,
            data_revision: row.try_get("data_revision").map_err(db_error)?,
            successful_sync_at: time(&row, "successful_sync_at")?,
            watermark: text(&row, "watermark")?.into(),
        }))
    }
}

#[cfg(test)]
impl SqliteRepository {
    /// Throwaway fault fixture: run one raw statement so a test can model a
    /// projection that was never committed, or an earlier backup that a restore
    /// put back. Production code never calls this.
    pub async fn execute_raw(&self, sql: &'static str) -> Result<(), StorageError> {
        sqlx::raw_sql(sql)
            .execute(&self.writer)
            .await
            .map_err(db_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
