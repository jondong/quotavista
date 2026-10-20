-- Phase 1 storage boundaries. No credential values or raw provider payloads.
CREATE TABLE store_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    data_revision INTEGER NOT NULL CHECK (data_revision >= 0)
) STRICT;
INSERT INTO store_state VALUES (1, 0);

CREATE TABLE settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL CHECK (revision >= 0),
    locale TEXT NOT NULL CHECK (locale IN ('en', 'zh-CN')),
    reporting_zone TEXT NOT NULL,
    currency TEXT NOT NULL,
    refresh_minutes INTEGER CHECK (refresh_minutes IN (1, 5, 15))
) STRICT;
INSERT INTO settings VALUES (1, 0, 'en', 'UTC', 'USD', NULL);

CREATE TABLE connections (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    alias TEXT NOT NULL,
    credential_ref TEXT,
    generation INTEGER NOT NULL CHECK (generation >= 0),
    lifecycle TEXT NOT NULL CHECK (lifecycle IN
        ('pending_setup', 'active', 'disabled', 'needs_credentials', 'deletion_pending')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX one_active_provider ON connections(provider) WHERE lifecycle = 'active';
CREATE TABLE provider_scopes (
    id TEXT PRIMARY KEY NOT NULL,
    connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    scope_key TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    UNIQUE (connection_id, scope_key, scope_type)
) STRICT;
CREATE TABLE products (
    id TEXT PRIMARY KEY NOT NULL,
    scope_id TEXT NOT NULL REFERENCES provider_scopes(id) ON DELETE CASCADE,
    product_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    valid_from TEXT,
    valid_until TEXT,
    period_key TEXT,
    rule_version TEXT,
    UNIQUE (scope_id, product_key),
    UNIQUE (scope_id, id)
) STRICT;

CREATE TABLE snapshots (
    scope_id TEXT NOT NULL REFERENCES provider_scopes(id) ON DELETE CASCADE,
    product_key TEXT NOT NULL,
    product_id TEXT,
    metric TEXT NOT NULL,
    value_kind TEXT NOT NULL,
    amount TEXT NOT NULL,
    unit_detail TEXT NOT NULL,
    provenance TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    source_as_of TEXT,
    successful_sync_at TEXT NOT NULL,
    data_revision INTEGER NOT NULL,
    FOREIGN KEY (scope_id, product_id) REFERENCES products(scope_id, id) ON DELETE CASCADE,
    CHECK (product_key = coalesce(product_id, '')),
    PRIMARY KEY (scope_id, product_key, metric)
) STRICT;
CREATE INDEX snapshot_observation ON snapshots(product_key, metric, observed_at);

CREATE TABLE partitions (
    id INTEGER PRIMARY KEY,
    scope_id TEXT NOT NULL REFERENCES provider_scopes(id) ON DELETE CASCADE,
    product_key TEXT NOT NULL,
    dataset TEXT NOT NULL,
    metric TEXT NOT NULL,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL CHECK (start_at < end_at),
    grain TEXT NOT NULL,
    dimensions TEXT NOT NULL,
    authority TEXT NOT NULL CHECK (authority IN ('synthetic', 'official_event', 'official_bucket')),
    UNIQUE (scope_id, product_key, dataset, metric, start_at, end_at, grain, dimensions, authority)
) STRICT;
CREATE TABLE sync_runs (
    id TEXT PRIMARY KEY NOT NULL,
    partition_id INTEGER NOT NULL REFERENCES partitions(id) ON DELETE CASCADE,
    generation INTEGER NOT NULL CHECK (generation >= 0),
    base_revision INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('incomplete', 'complete', 'published')),
    cursor TEXT,
    published_revision INTEGER
) STRICT;
CREATE TABLE staged_facts (
    run_id TEXT NOT NULL REFERENCES sync_runs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('event', 'bucket')),
    event_id TEXT NOT NULL,
    key_start TEXT NOT NULL,
    key_end TEXT NOT NULL,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL CHECK (start_at < end_at),
    value_kind TEXT NOT NULL,
    amount TEXT NOT NULL,
    unit_detail TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    CHECK ((kind = 'event' AND event_id <> '' AND key_start = '' AND key_end = '') OR
           (kind = 'bucket' AND event_id = '' AND key_start = start_at AND key_end = end_at)),
    PRIMARY KEY (run_id, kind, event_id, key_start, key_end)
) STRICT;
CREATE TABLE published_partitions (
    partition_id INTEGER PRIMARY KEY REFERENCES partitions(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES sync_runs(id) ON DELETE CASCADE,
    data_revision INTEGER NOT NULL,
    successful_sync_at TEXT NOT NULL,
    watermark TEXT NOT NULL
) STRICT;
CREATE TABLE usage_facts (
    partition_id INTEGER NOT NULL REFERENCES partitions(id) ON DELETE CASCADE,
    scope_id TEXT NOT NULL REFERENCES provider_scopes(id) ON DELETE CASCADE,
    dataset TEXT NOT NULL,
    metric TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('event', 'bucket')),
    event_id TEXT NOT NULL,
    key_start TEXT NOT NULL,
    key_end TEXT NOT NULL,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL CHECK (start_at < end_at),
    value_kind TEXT NOT NULL,
    amount TEXT NOT NULL,
    unit_detail TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    PRIMARY KEY (partition_id, kind, event_id, key_start, key_end)
) STRICT;
CREATE UNIQUE INDEX event_identity ON usage_facts(scope_id, dataset, event_id, metric) WHERE kind = 'event';
CREATE INDEX usage_interval ON usage_facts(scope_id, metric, start_at, end_at);
CREATE TABLE coverage_intervals (
    partition_id INTEGER PRIMARY KEY REFERENCES partitions(id) ON DELETE CASCADE,
    source_zone TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('complete', 'partial', 'missing', 'unsupported')),
    reason TEXT,
    data_revision INTEGER NOT NULL
) STRICT;
CREATE TABLE sync_checkpoints (
    partition_id INTEGER PRIMARY KEY REFERENCES partitions(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES sync_runs(id) ON DELETE CASCADE,
    watermark TEXT NOT NULL,
    data_revision INTEGER NOT NULL
) STRICT;

-- Metadata boundaries only: orchestration, alert delivery, and managed recovery are later tasks.
CREATE TABLE lifecycle_operations (
    id TEXT PRIMARY KEY NOT NULL,
    connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    candidate_ref TEXT,
    previous_ref TEXT,
    generation INTEGER NOT NULL
) STRICT;
CREATE INDEX lifecycle_state ON lifecycle_operations(state);
CREATE TABLE alert_states (
    rule_id TEXT NOT NULL,
    scope_id TEXT NOT NULL REFERENCES provider_scopes(id) ON DELETE CASCADE,
    product_key TEXT NOT NULL,
    period_key TEXT NOT NULL,
    episode_sequence INTEGER NOT NULL,
    last_metric_revision INTEGER NOT NULL,
    notification_id TEXT,
    delivery_state TEXT NOT NULL,
    PRIMARY KEY (rule_id, scope_id, product_key, period_key)
) STRICT;
CREATE TABLE managed_backups (
    id TEXT PRIMARY KEY NOT NULL,
    schema_version INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    state TEXT NOT NULL
) STRICT;
-- Opaque membership deliberately survives live-row deletion until backup cleanup.
CREATE TABLE managed_backup_scopes (
    backup_id TEXT NOT NULL REFERENCES managed_backups(id) ON DELETE CASCADE,
    opaque_scope_id TEXT NOT NULL,
    PRIMARY KEY (backup_id, opaque_scope_id)
) STRICT;
