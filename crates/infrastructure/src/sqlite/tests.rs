//! Synthetic fixtures only. Every database and migration directory is throwaway.
use super::*;
use sqlx::migrate::{Migration, MigrationType};
use std::{borrow::Cow, time::Instant};
use tempfile::TempDir;

fn at(hour: u32) -> DateTime<Utc> {
    format!("2026-01-01T{hour:02}:00:00Z").parse().unwrap()
}
fn context() -> WriteContext {
    WriteContext {
        connection_id: "synthetic-connection".into(),
        generation: 0,
    }
}
fn connection() -> Connection {
    Connection {
        id: context().connection_id,
        provider: "synthetic-provider".into(),
        alias: "Synthetic".into(),
        generation: 0,
        lifecycle: Lifecycle::Active,
    }
}
fn scope() -> Scope {
    Scope {
        id: "synthetic-scope".into(),
        connection_id: context().connection_id,
        key: "synthetic-account".into(),
        kind: "account".into(),
    }
}
fn partition() -> Partition {
    Partition {
        scope_id: scope().id,
        product_key: String::new(),
        dataset: "synthetic-usage".into(),
        metric: "tokens".into(),
        interval: Interval::new(at(0), at(12)).unwrap(),
        grain: "event".into(),
        dimensions: String::new(),
        authority: Provenance::Synthetic,
    }
}
fn event(id: &str, hour: u32, amount: u64) -> Fact {
    Fact {
        identity: FactIdentity::Event(id.into()),
        interval: Interval::new(at(hour), at(hour + 1)).unwrap(),
        value: Value::Tokens(TokenTotal(amount.into())),
        source_revision: "synthetic-v1".into(),
    }
}
fn snapshot(value: Value) -> Snapshot {
    Snapshot {
        scope_id: scope().id,
        product_id: None,
        metric: "synthetic-value".into(),
        value,
        provenance: Provenance::Synthetic,
        observed_at: at(1),
        source_as_of: Some(at(0)),
    }
}
async fn fixture() -> (TempDir, SqliteRepository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    repo.create_connection(&connection(), &scope())
        .await
        .unwrap();
    (dir, repo)
}
async fn complete(repo: &SqliteRepository, id: &str, p: &Partition, facts: &[Fact]) {
    repo.begin_candidate(&context(), id, p).await.unwrap();
    repo.stage_page(&context(), id, None, "synthetic-terminal", facts)
        .await
        .unwrap();
    repo.complete_candidate(&context(), id, Some("synthetic-terminal"))
        .await
        .unwrap();
}
async fn count(repo: &SqliteRepository, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(&repo.readers)
        .await
        .unwrap()
}
async fn execute(repo: &SqliteRepository, sql: &'static str) {
    sqlx::raw_sql(sql).execute(&repo.writer).await.unwrap();
}

#[tokio::test]
async fn fresh_migration_reopen_settings_and_exact_values() {
    let (dir, repo) = fixture().await;
    let mut settings = repo.settings().await.unwrap();
    assert_eq!(settings.revision, 0);
    settings.locale = "zh-CN".into();
    settings.reporting_zone = "America/New_York".parse().unwrap();
    settings.currency = "EUR".parse().unwrap();
    settings.refresh_minutes = Some(15);
    let saved = repo.save_settings(&settings).await.unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(
        repo.save_settings(&settings).await,
        Err(StorageError::Conflict)
    );
    settings.locale = "invalid".into();
    assert_eq!(
        repo.save_settings(&settings).await,
        Err(StorageError::Validation)
    );
    let values = [
        Value::Tokens(TokenTotal(u128::MAX)),
        Value::Tokens(TokenTotal(u64::MAX.into())),
        Value::Tokens(TokenTotal(0)),
        Value::Money(Money {
            amount: "1234567890.123456789012345678".parse().unwrap(),
            currency: "USD".parse().unwrap(),
        }),
        Value::Ratio("0.123456789012345678".parse().unwrap()),
        Value::PercentagePoints("100.123456789012345678".parse().unwrap()),
        Value::Native {
            amount: "0.000000000000000001".parse().unwrap(),
            unit: "synthetic-credit".into(),
        },
    ];
    for (i, value) in values.iter().enumerate() {
        let mut s = snapshot(value.clone());
        s.metric = format!("synthetic-{i}");
        repo.save_snapshot(&context(), &s, at(2)).await.unwrap();
    }
    assert_eq!(
        count(
            &repo,
            "SELECT count(*) FROM snapshots WHERE typeof(amount)='text'"
        )
        .await,
        values.len() as i64
    );
    repo.close().await;
    let repo = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    assert_eq!(repo.settings().await.unwrap(), saved);
    assert_eq!(
        repo.connection(&context().connection_id).await.unwrap(),
        Some(connection())
    );
    for (i, value) in values.into_iter().enumerate() {
        let stored = repo
            .snapshot(&scope().id, None, &format!("synthetic-{i}"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.snapshot.value, value);
        assert_eq!(stored.snapshot.observed_at, at(1));
        assert_eq!(stored.snapshot.source_as_of, Some(at(0)));
        assert_eq!(stored.snapshot.provenance, Provenance::Synthetic);
        assert_eq!(stored.successful_sync_at, at(2));
        assert_eq!(stored.data_revision, i as i64 + 1);
    }
    assert_eq!(
        count(
            &repo,
            "SELECT count(*) FROM _sqlx_migrations WHERE success=1"
        )
        .await,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&repo.readers)
            .await
            .unwrap(),
        "ok"
    );
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&repo.readers)
        .await
        .unwrap()
        .is_empty());
    repo.close().await;
}

#[tokio::test]
async fn failed_snapshot_rolls_back_value_success_time_and_revision() {
    let (_dir, repo) = fixture().await;
    let old = snapshot(Value::Tokens(TokenTotal(0)));
    repo.save_snapshot(&context(), &old, at(2)).await.unwrap();
    let before = repo.snapshot(&scope().id, None, &old.metric).await.unwrap();
    execute(&repo, "CREATE TRIGGER synthetic_fail_snapshot BEFORE UPDATE ON snapshots BEGIN SELECT RAISE(ABORT,'synthetic fault'); END;").await;
    let changed = snapshot(Value::Tokens(TokenTotal(u128::MAX)));
    assert!(repo
        .save_snapshot(&context(), &changed, at(3))
        .await
        .is_err());
    assert_eq!(
        repo.snapshot(&scope().id, None, &old.metric).await.unwrap(),
        before
    );
    assert_eq!(
        count(&repo, "SELECT data_revision FROM store_state").await,
        1
    );
    execute(&repo, "DROP TRIGGER synthetic_fail_snapshot").await;
    // A bad product FK fails after the transaction's revision increment, too.
    let mut invalid = changed.clone();
    invalid.product_id = Some("synthetic-missing-product".into());
    assert_eq!(
        repo.save_snapshot(&context(), &invalid, at(3)).await,
        Err(StorageError::Conflict)
    );
    assert_eq!(
        count(&repo, "SELECT data_revision FROM store_state").await,
        1
    );
    repo.close().await;
}

#[tokio::test]
async fn connections_lists_persisted_rows_in_id_order() {
    let (_dir, repo) = fixture().await;
    // Fixture seeds one connection (`context().connection_id`).
    let one = repo.connections().await.unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].id, connection().id);

    // A second connection adds a row and ordering stays stable.
    let mut second = connection();
    second.id = "synthetic-connection-other".into();
    second.alias = "other".into();
    second.lifecycle = Lifecycle::Disabled;
    let mut scope_other = scope();
    scope_other.id = "synthetic-scope-other".into();
    scope_other.connection_id = "synthetic-connection-other".into();
    repo.create_connection(&second, &scope_other).await.unwrap();
    let listed = repo.connections().await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, connection().id);
    assert_eq!(listed[1].id, "synthetic-connection-other");
    repo.close().await;
}

#[tokio::test]
async fn candidate_page_cursor_rollback_restart_and_partial_replacement() {
    let (dir, repo) = fixture().await;
    let p = partition();
    complete(&repo, "synthetic-old", &p, &[event("a", 1, 7)]).await;
    repo.publish(&context(), "synthetic-old", at(3), "old-watermark")
        .await
        .unwrap();
    let old = repo.published(&p).await.unwrap();
    repo.begin_candidate(&context(), "synthetic-new", &p)
        .await
        .unwrap();
    repo.stage_page(
        &context(),
        "synthetic-new",
        None,
        "page-1",
        &[event("b", 2, u64::MAX)],
    )
    .await
    .unwrap();
    execute(&repo, "CREATE TRIGGER synthetic_fail_cursor BEFORE UPDATE OF cursor ON sync_runs BEGIN SELECT RAISE(ABORT,'synthetic fault'); END;").await;
    assert!(repo
        .stage_page(
            &context(),
            "synthetic-new",
            Some("page-1"),
            "page-2",
            &[event("c", 3, 9)]
        )
        .await
        .is_err());
    assert_eq!(
        repo.candidate_cursor("synthetic-new")
            .await
            .unwrap()
            .as_deref(),
        Some("page-1")
    );
    assert_eq!(count(&repo, "SELECT count(*) FROM staged_facts").await, 1);
    assert_eq!(repo.published(&p).await.unwrap(), old);
    assert_eq!(
        repo.publish(&context(), "synthetic-new", at(4), "new")
            .await,
        Err(StorageError::Conflict)
    );
    execute(&repo, "DROP TRIGGER synthetic_fail_cursor").await;
    repo.close().await;
    let repo = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    assert_eq!(
        repo.candidate_cursor("synthetic-new")
            .await
            .unwrap()
            .as_deref(),
        Some("page-1")
    );
    assert_eq!(repo.published(&p).await.unwrap(), old);
    // A duplicate event on a later page corrects its timestamp without adding a row.
    repo.stage_page(
        &context(),
        "synthetic-new",
        Some("page-1"),
        "page-2",
        &[event("b", 4, 11)],
    )
    .await
    .unwrap();
    assert_eq!(
        repo.stage_page(&context(), "synthetic-new", Some("page-1"), "page-3", &[])
            .await,
        Err(StorageError::Conflict)
    );
    assert_eq!(
        repo.stage_page(&context(), "synthetic-new", Some("page-2"), "page-2", &[])
            .await,
        Err(StorageError::Validation)
    );
    repo.complete_candidate(&context(), "synthetic-new", Some("page-2"))
        .await
        .unwrap();
    let rev = repo
        .publish(&context(), "synthetic-new", at(5), "new-watermark")
        .await
        .unwrap();
    let new = repo.published(&p).await.unwrap().unwrap();
    assert_eq!(new.facts, vec![event("b", 4, 11)]);
    assert_eq!(new.successful_sync_at, at(5));
    assert_eq!(
        repo.publish(&context(), "synthetic-new", at(6), "ignored")
            .await
            .unwrap(),
        rev
    );
    assert_eq!(repo.published(&p).await.unwrap(), Some(new));
    assert_eq!(count(&repo, "SELECT count(*) FROM staged_facts").await, 0);
    assert_eq!(
        count(&repo, "SELECT data_revision FROM coverage_intervals").await,
        rev
    );
    assert_eq!(
        count(&repo, "SELECT data_revision FROM sync_checkpoints").await,
        rev
    );
    // Authoritatively empty is different from incomplete.
    complete(&repo, "synthetic-empty", &p, &[]).await;
    repo.publish(&context(), "synthetic-empty", at(7), "empty-watermark")
        .await
        .unwrap();
    assert!(repo.published(&p).await.unwrap().unwrap().facts.is_empty());
    repo.close().await;
}

#[tokio::test]
async fn publication_failure_after_delete_rolls_back_every_boundary() {
    let (_dir, repo) = fixture().await;
    let p = partition();
    complete(
        &repo,
        "synthetic-old",
        &p,
        &[event("a", 1, 1), event("b", 2, 2)],
    )
    .await;
    repo.publish(&context(), "synthetic-old", at(3), "old")
        .await
        .unwrap();
    let old = repo.published(&p).await.unwrap();
    complete(&repo, "synthetic-new", &p, &[event("a", 4, 99)]).await;
    // Fails after DELETE, replacement INSERT, pointer, revision, and coverage writes.
    execute(&repo, "CREATE TRIGGER synthetic_fail_checkpoint BEFORE UPDATE ON sync_checkpoints BEGIN SELECT RAISE(ABORT,'synthetic fault'); END;").await;
    assert!(repo
        .publish(&context(), "synthetic-new", at(5), "new")
        .await
        .is_err());
    assert_eq!(repo.published(&p).await.unwrap(), old);
    assert_eq!(
        count(&repo, "SELECT data_revision FROM store_state").await,
        1
    );
    assert_eq!(
        count(&repo, "SELECT data_revision FROM coverage_intervals").await,
        1
    );
    assert_eq!(
        count(&repo, "SELECT data_revision FROM sync_checkpoints").await,
        1
    );
    assert_eq!(count(&repo, "SELECT count(*) FROM staged_facts").await, 1);
    execute(&repo, "DROP TRIGGER synthetic_fail_checkpoint").await;
    repo.publish(&context(), "synthetic-new", at(5), "new")
        .await
        .unwrap();
    assert_eq!(
        repo.published(&p).await.unwrap().unwrap().facts,
        vec![event("a", 4, 99)]
    );
    repo.close().await;
}

#[tokio::test]
async fn rejects_stale_generations_foreign_scopes_and_older_candidates() {
    let (dir, repo) = fixture().await;
    let p = partition();
    complete(&repo, "synthetic-a", &p, &[event("a", 1, 1)]).await;
    complete(&repo, "synthetic-b", &p, &[event("b", 2, 2)]).await;
    repo.publish(&context(), "synthetic-b", at(3), "new")
        .await
        .unwrap();
    assert_eq!(
        repo.publish(&context(), "synthetic-a", at(4), "old").await,
        Err(StorageError::Conflict)
    );
    let mut other = context();
    other.connection_id = "synthetic-foreign-connection".into();
    assert_eq!(
        repo.save_snapshot(&other, &snapshot(Value::Tokens(TokenTotal(1))), at(4))
            .await,
        Err(StorageError::Conflict)
    );
    // A second independently opened writer commits a lifecycle mutation first.
    let other = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    assert_eq!(
        other
            .set_lifecycle(&context(), Lifecycle::Disabled)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repo.stage_page(
            &context(),
            "synthetic-a",
            Some("synthetic-terminal"),
            "late",
            &[]
        )
        .await,
        Err(StorageError::Conflict)
    );
    assert_eq!(
        repo.complete_candidate(&context(), "synthetic-a", Some("synthetic-terminal"))
            .await,
        Err(StorageError::Conflict)
    );
    assert_eq!(
        repo.publish(&context(), "synthetic-a", at(5), "late").await,
        Err(StorageError::Conflict)
    );
    assert_eq!(
        repo.save_snapshot(&context(), &snapshot(Value::Tokens(TokenTotal(1))), at(5))
            .await,
        Err(StorageError::Conflict)
    );
    assert_eq!(
        repo.published(&p).await.unwrap().unwrap().facts,
        vec![event("b", 2, 2)]
    );
    other.close().await;
    repo.close().await;
}

#[tokio::test]
async fn event_identity_correction_and_cross_partition_conflict() {
    let (_dir, repo) = fixture().await;
    let p = partition();
    complete(&repo, "synthetic-first", &p, &[event("same-id", 1, 1)]).await;
    repo.publish(&context(), "synthetic-first", at(3), "first")
        .await
        .unwrap();
    complete(&repo, "synthetic-corrected", &p, &[event("same-id", 4, 5)]).await;
    repo.publish(&context(), "synthetic-corrected", at(6), "corrected")
        .await
        .unwrap();
    assert_eq!(count(&repo, "SELECT count(*) FROM usage_facts").await, 1);
    assert_eq!(
        repo.published(&p).await.unwrap().unwrap().facts,
        vec![event("same-id", 4, 5)]
    );
    let mut next = p.clone();
    next.interval = Interval::new(at(12), at(23)).unwrap();
    complete(&repo, "synthetic-next", &next, &[event("other-id", 13, 3)]).await;
    repo.publish(&context(), "synthetic-next", at(15), "next")
        .await
        .unwrap();
    let before = repo.published(&next).await.unwrap();
    complete(&repo, "synthetic-cross", &next, &[event("same-id", 16, 7)]).await;
    assert_eq!(
        repo.publish(&context(), "synthetic-cross", at(18), "cross")
            .await,
        Err(StorageError::Conflict)
    );
    assert_eq!(repo.published(&next).await.unwrap(), before);
    assert_eq!(count(&repo, "SELECT count(*) FROM usage_facts").await, 2);
    repo.close().await;
}

#[tokio::test]
async fn bucket_keys_replacement_and_overlap_authority_guard() {
    let (_dir, repo) = fixture().await;
    let mut p = partition();
    p.grain = "hour".into();
    p.authority = Provenance::OfficialBucket;
    let mut a = event("unused", 1, 1);
    a.identity = FactIdentity::Bucket;
    let mut b = a.clone();
    b.interval = Interval::new(at(2), at(3)).unwrap();
    complete(
        &repo,
        "synthetic-buckets",
        &p,
        &[a.clone(), a.clone(), b.clone()],
    )
    .await;
    repo.publish(&context(), "synthetic-buckets", at(4), "first")
        .await
        .unwrap();
    assert_eq!(
        repo.published(&p).await.unwrap().unwrap().facts,
        vec![a.clone(), b]
    );
    a.value = Value::Tokens(TokenTotal(9));
    complete(&repo, "synthetic-replace-bucket", &p, &[a.clone()]).await;
    repo.publish(&context(), "synthetic-replace-bucket", at(5), "second")
        .await
        .unwrap();
    assert_eq!(repo.published(&p).await.unwrap().unwrap().facts, vec![a]);
    let overlapping = partition();
    complete(
        &repo,
        "synthetic-overlap",
        &overlapping,
        &[event("event", 1, 1)],
    )
    .await;
    assert_eq!(
        repo.publish(&context(), "synthetic-overlap", at(6), "overlap")
            .await,
        Err(StorageError::Conflict)
    );
    assert!(repo.published(&overlapping).await.unwrap().is_none());
    repo.close().await;
}

#[tokio::test]
async fn pooled_settings_linked_version_and_bounded_busy_error() {
    let (_dir, repo) = fixture().await;
    // Holding all acquisitions simultaneously proves settings on distinct connections,
    // not repeated checkout of the first member of the pool.
    let mut readers = Vec::new();
    for _ in 0..READ_CONNECTIONS {
        readers.push(repo.readers.acquire().await.unwrap());
    }
    let mut writer = repo.writer.acquire().await.unwrap();
    for conn in readers.iter_mut().chain(std::iter::once(&mut writer)) {
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT sqlite_version()")
                .fetch_one(&mut **conn)
                .await
                .unwrap(),
            SQLITE_VERSION
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
                .fetch_one(&mut **conn)
                .await
                .unwrap(),
            "wal"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
                .fetch_one(&mut **conn)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA busy_timeout")
                .fetch_one(&mut **conn)
                .await
                .unwrap(),
            BUSY_TIMEOUT.as_millis() as i64
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA synchronous")
                .fetch_one(&mut **conn)
                .await
                .unwrap(),
            2
        );
    }
    drop(readers);
    drop(writer);
    let options = repo.writer.connect_options();
    let external = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with((*options).clone())
        .await
        .unwrap();
    let lock = external.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let started = Instant::now();
    let result = repo
        .save_snapshot(&context(), &snapshot(Value::Tokens(TokenTotal(1))), at(3))
        .await;
    let elapsed = started.elapsed();
    assert_eq!(result, Err(StorageError::Unavailable));
    // SQLite budgets requested sleeps, not wall time; loaded runners need scheduling slack.
    assert!(
        elapsed >= Duration::from_secs(4) && elapsed < Duration::from_secs(20),
        "{elapsed:?}"
    );
    lock.rollback().await.unwrap();
    assert!(repo
        .snapshot(&scope().id, None, "synthetic-value")
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        count(&repo, "SELECT data_revision FROM store_state").await,
        0
    );
    external.close().await;
    repo.close().await;
}

#[tokio::test]
async fn migration_failure_is_transactional_and_populated_upgrades_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-migration.sqlite");
    let bad = Migration::new(1, Cow::Borrowed("synthetic fault"), MigrationType::Simple,
        sqlx::SqlStr::from_static("CREATE TABLE synthetic_sentinel(value TEXT); INSERT INTO synthetic_sentinel VALUES('synthetic'); INVALID SYNTHETIC SQL;"), false);
    let migrator = Migrator {
        migrations: Cow::Owned(vec![bad]),
        ..Migrator::DEFAULT
    };
    assert!(matches!(
        SqliteRepository::open_with_migrator(&path, &migrator).await,
        Err(StorageError::Migration)
    ));
    let repo = SqliteRepository::open(&path).await.unwrap();
    assert_eq!(
        count(
            &repo,
            "SELECT count(*) FROM sqlite_master WHERE name='synthetic_sentinel'"
        )
        .await,
        0
    );
    let mut settings = repo.settings().await.unwrap();
    settings.locale = "zh-CN".into();
    let saved = repo.save_settings(&settings).await.unwrap();
    repo.close().await;
    let mut changed = MIGRATOR.iter().cloned().collect::<Vec<_>>();
    changed[0] = Migration::new(
        1,
        Cow::Borrowed("synthetic changed checksum"),
        MigrationType::Simple,
        sqlx::SqlStr::from_static("DROP TABLE settings;"),
        false,
    );
    let changed = Migrator {
        migrations: Cow::Owned(changed),
        ..Migrator::DEFAULT
    };
    assert!(matches!(
        SqliteRepository::open_with_migrator(&path, &changed).await,
        Err(StorageError::Migration)
    ));
    let upgrade = Migration::new(
        2,
        Cow::Borrowed("synthetic upgrade"),
        MigrationType::Simple,
        sqlx::SqlStr::from_static("DROP TABLE settings;"),
        false,
    );
    let mut migrations = MIGRATOR.iter().cloned().collect::<Vec<_>>();
    migrations.push(upgrade);
    let migrator = Migrator {
        migrations: Cow::Owned(migrations),
        ..Migrator::DEFAULT
    };
    assert!(matches!(
        SqliteRepository::open_with_migrator(&path, &migrator).await,
        Err(StorageError::Migration)
    ));
    let repo = SqliteRepository::open(&path).await.unwrap();
    assert_eq!(repo.settings().await.unwrap(), saved);
    repo.close().await;
}

#[tokio::test]
async fn trust_boundary_constraints_and_canonical_decode() {
    let (_dir, repo) = fixture().await;
    for alias in ["", " Synthetic", "Synthetic\nAlias", "Synthetic\0Alias"] {
        let mut invalid = connection();
        invalid.alias = alias.into();
        assert_eq!(
            repo.create_connection(&invalid, &scope()).await,
            Err(StorageError::Validation)
        );
    }
    assert_eq!(count(&repo, "SELECT count(*) FROM connections").await, 1);
    let mut duplicate = connection();
    duplicate.id = "synthetic-duplicate".into();
    let mut duplicate_scope = scope();
    duplicate_scope.id = "synthetic-other-scope".into();
    duplicate_scope.connection_id = duplicate.id.clone();
    assert_eq!(
        repo.create_connection(&duplicate, &duplicate_scope).await,
        Err(StorageError::Conflict)
    );
    assert_eq!(count(&repo, "SELECT count(*) FROM connections").await, 1);
    let mut too_large = event("overflow", 1, 1);
    too_large.value = Value::Tokens(TokenTotal(u128::from(u64::MAX) + 1));
    repo.begin_candidate(&context(), "synthetic-invalid", &partition())
        .await
        .unwrap();
    assert_eq!(
        repo.stage_page(
            &context(),
            "synthetic-invalid",
            None,
            "cursor",
            &[event("valid", 1, 1), too_large]
        )
        .await,
        Err(StorageError::Validation)
    );
    assert_eq!(count(&repo, "SELECT count(*) FROM staged_facts").await, 0);
    assert_eq!(
        repo.candidate_cursor("synthetic-invalid").await.unwrap(),
        None
    );
    repo.save_snapshot(&context(), &snapshot(Value::Tokens(TokenTotal(1))), at(2))
        .await
        .unwrap();
    execute(&repo, "UPDATE snapshots SET amount='01'").await;
    assert_eq!(
        repo.snapshot(&scope().id, None, "synthetic-value").await,
        Err(StorageError::Unavailable)
    );
    execute(&repo, "INSERT INTO lifecycle_operations VALUES('synthetic-op','synthetic-connection','rotation','pending',NULL,NULL,0)").await;
    assert_eq!(
        repo.save_snapshot(&context(), &snapshot(Value::Tokens(TokenTotal(2))), at(3))
            .await,
        Err(StorageError::Conflict)
    );
    repo.close().await;
}

#[tokio::test]
async fn independent_writers_race_publications_without_mixing_revisions() {
    let (dir, first) = fixture().await;
    let second = SqliteRepository::open(&dir.path().join("synthetic.sqlite"))
        .await
        .unwrap();
    let p = partition();
    complete(&first, "synthetic-race-a", &p, &[event("a", 1, 1)]).await;
    complete(&second, "synthetic-race-b", &p, &[event("b", 2, 2)]).await;
    let ctx = context();
    let (a, b) = tokio::join!(
        first.publish(&ctx, "synthetic-race-a", at(3), "a"),
        second.publish(&ctx, "synthetic-race-b", at(4), "b"),
    );
    let (expected, success_time, watermark) = match (a, b) {
        (Ok(1), Err(StorageError::Conflict)) => (event("a", 1, 1), at(3), "a"),
        (Err(StorageError::Conflict), Ok(1)) => (event("b", 2, 2), at(4), "b"),
        results => panic!("unexpected publication race: {results:?}"),
    };
    let published = first.published(&p).await.unwrap().unwrap();
    assert_eq!(published.facts, vec![expected]);
    assert_eq!(published.data_revision, 1);
    assert_eq!(published.successful_sync_at, success_time);
    assert_eq!(published.watermark, watermark);
    assert_eq!(second.published(&p).await.unwrap(), Some(published));
    assert_eq!(
        count(&first, "SELECT data_revision FROM store_state").await,
        1
    );
    assert_eq!(
        count(&first, "SELECT data_revision FROM coverage_intervals").await,
        1
    );
    assert_eq!(
        count(&first, "SELECT data_revision FROM sync_checkpoints").await,
        1
    );
    assert_eq!(count(&first, "SELECT count(*) FROM staged_facts").await, 1);
    assert_eq!(
        count(
            &first,
            "SELECT count(*) FROM sync_runs WHERE state='complete'"
        )
        .await,
        1
    );
    second.close().await;
    first.close().await;
}

#[tokio::test]
async fn statistics_reads_round_trip_snapshots_and_published_partitions() {
    let (_dir, repo) = fixture().await;
    repo.save_snapshot(&context(), &snapshot(Value::Tokens(TokenTotal(7))), at(2))
        .await
        .unwrap();
    let p = Partition {
        grain: "P1D".into(),
        interval: Interval::new(at(0), at(0) + chrono::Duration::days(2)).unwrap(),
        ..partition()
    };
    let facts = vec![
        Fact {
            identity: FactIdentity::Bucket,
            interval: Interval::new(at(0), at(0) + chrono::Duration::days(1)).unwrap(),
            value: Value::Tokens(TokenTotal(41)),
            source_revision: "synthetic-v1".into(),
        },
        Fact {
            identity: FactIdentity::Bucket,
            interval: Interval::new(
                at(0) + chrono::Duration::days(1),
                at(0) + chrono::Duration::days(2),
            )
            .unwrap(),
            value: Value::Tokens(TokenTotal(42)),
            source_revision: "synthetic-v1".into(),
        },
    ];
    complete(&repo, "synthetic-stats", &p, &facts).await;
    let revision = repo
        .publish(&context(), "synthetic-stats", at(3), "synthetic-wm")
        .await
        .unwrap();

    let snapshots = repo.snapshots(&[scope().id]).await.unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].snapshot,
        snapshot(Value::Tokens(TokenTotal(7)))
    );
    assert_eq!(snapshots[0].data_revision, 1);

    let read = repo
        .published_partitions(
            &[scope().id],
            &Interval::new(
                at(0) - chrono::Duration::days(1),
                at(0) + chrono::Duration::days(3),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read.len(), 1);
    let stored = &read[0];
    assert_eq!(stored.partition, p);
    assert_eq!(stored.facts, facts);
    assert_eq!(stored.data_revision, revision);
    assert_eq!(stored.successful_sync_at, at(3));
    assert_eq!(stored.watermark, "synthetic-wm");
    assert_eq!(stored.coverage.len(), 1);
    assert_eq!(stored.coverage[0].zone, "UTC");
    assert_eq!(stored.coverage[0].status, CoverageStatus::Complete);
    assert_eq!(stored.coverage[0].reason, None);
    assert_eq!(stored.coverage[0].interval, p.interval);

    // Ranges that do not overlap the published partition read nothing.
    let outside = repo
        .published_partitions(
            &[scope().id],
            &Interval::new(
                at(0) + chrono::Duration::days(30),
                at(0) + chrono::Duration::days(31),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(outside.is_empty());
    // Empty scope lists short-circuit without touching the store.
    assert!(repo.snapshots(&[]).await.unwrap().is_empty());
    assert!(repo
        .published_partitions(&[], &p.interval)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn statistics_read_refuses_an_unbounded_fact_scan() {
    let (_dir, repo) = fixture().await;
    let p = Partition {
        grain: "P1D".into(),
        ..partition()
    };
    complete(&repo, "synthetic-bulk", &p, &[]).await;
    repo.publish(&context(), "synthetic-bulk", at(3), "synthetic-wm")
        .await
        .unwrap();
    // Bulk-insert 100'001 synthetic bucket facts with distinct key bounds.
    execute(
        &repo,
        "INSERT INTO usage_facts WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<100001) SELECT 1,'synthetic-scope','synthetic-usage','tokens','bucket','',printf('%06d',n),printf('%06d',n),'2026-01-01T00:00:00Z','2026-01-01T01:00:00Z','tokens','5','','synthetic-v1' FROM seq",
    )
    .await;
    let result = repo.published_partitions(&[scope().id], &p.interval).await;
    assert_eq!(result, Err(StorageError::ResourceLimit));
}
