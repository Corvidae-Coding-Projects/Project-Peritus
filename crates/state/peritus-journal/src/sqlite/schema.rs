//! Immutable schema-version-eight SQL.

pub(super) const SCHEMA_VERSION: i64 = 8;

pub(super) const INSTALL_SCHEMA: &str = r"
BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS store_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    store_id BLOB NOT NULL CHECK (length(store_id) = 16),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0)
) STRICT;
CREATE TABLE IF NOT EXISTS aggregate_heads (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 15),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS events (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 15),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    previous_event_id BLOB CHECK (previous_event_id IS NULL OR length(previous_event_id) = 16),
    previous_event_hash BLOB NOT NULL CHECK (length(previous_event_hash) = 32),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    command_id BLOB NOT NULL CHECK (length(command_id) = 16),
    frame_family INTEGER NOT NULL CHECK (frame_family > 0),
    frame_schema INTEGER NOT NULL CHECK (frame_schema > 0),
    frame_digest BLOB NOT NULL CHECK (length(frame_digest) = 32),
    revision_digest BLOB NOT NULL CHECK (length(revision_digest) = 32),
    causal_ids BLOB NOT NULL CHECK ((length(causal_ids) % 16) = 0),
    frame BLOB NOT NULL,
    UNIQUE (aggregate_kind, aggregate_id, sequence)
) STRICT;
CREATE INDEX IF NOT EXISTS events_command ON events(command_id, global_position);
CREATE TABLE IF NOT EXISTS commands (
    command_id BLOB PRIMARY KEY CHECK (length(command_id) = 16),
    request_digest BLOB NOT NULL CHECK (length(request_digest) = 32),
    first_position INTEGER NOT NULL CHECK (first_position > 0),
    last_position INTEGER NOT NULL CHECK (last_position >= first_position),
    event_count INTEGER NOT NULL CHECK (event_count > 0),
    batch_hash BLOB NOT NULL UNIQUE CHECK (length(batch_hash) = 32)
) STRICT, WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS state_records (
    namespace INTEGER NOT NULL CHECK (namespace > 0),
    record_key BLOB NOT NULL CHECK (length(record_key) BETWEEN 1 AND 1024),
    revision INTEGER NOT NULL CHECK (revision > 0),
    value_digest BLOB NOT NULL CHECK (length(value_digest) = 32),
    value BLOB NOT NULL,
    producing_position INTEGER NOT NULL REFERENCES events(global_position),
    PRIMARY KEY (namespace, record_key)
) STRICT, WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS state_record_history (
    namespace INTEGER NOT NULL CHECK (namespace > 0),
    record_key BLOB NOT NULL CHECK (length(record_key) BETWEEN 1 AND 1024),
    revision INTEGER NOT NULL CHECK (revision > 0),
    value_digest BLOB NOT NULL CHECK (length(value_digest) = 32),
    value BLOB NOT NULL,
    producing_position INTEGER NOT NULL REFERENCES events(global_position),
    PRIMARY KEY (namespace, record_key, revision)
) STRICT, WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS state_history_producer
    ON state_record_history(producing_position, namespace, record_key);
CREATE TABLE IF NOT EXISTS outbox (
    outbox_id BLOB PRIMARY KEY CHECK (length(outbox_id) = 16),
    producing_position INTEGER NOT NULL REFERENCES events(global_position),
    destination TEXT NOT NULL CHECK (length(destination) BETWEEN 1 AND 512),
    payload BLOB NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts > 0),
    state INTEGER NOT NULL DEFAULT 1 CHECK (state BETWEEN 1 AND 4),
    fence INTEGER CHECK (fence IS NULL OR fence > 0),
    lease_until INTEGER CHECK (lease_until IS NULL OR lease_until > 0)
) STRICT, WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS outbox_delivery ON outbox(state, lease_until, outbox_id);
CREATE TABLE IF NOT EXISTS authority_clock (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    current_epoch INTEGER NOT NULL CHECK (current_epoch > 0)
) STRICT;
CREATE TABLE IF NOT EXISTS credential_registry (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    revision INTEGER NOT NULL CHECK (revision > 0),
    generation INTEGER NOT NULL CHECK (generation > 0),
    snapshot_digest BLOB NOT NULL CHECK (length(snapshot_digest) = 32),
    snapshot BLOB NOT NULL,
    producing_position INTEGER NOT NULL REFERENCES events(global_position)
) STRICT;
COMMIT;
";
