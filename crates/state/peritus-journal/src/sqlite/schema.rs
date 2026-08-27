//! Immutable schema-version-ten SQL.

pub(super) const SCHEMA_VERSION: i64 = 10;

pub(super) const INSTALL_SCHEMA: &str = r"
BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS store_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    store_id BLOB NOT NULL CHECK (length(store_id) = 16),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0)
) STRICT;
CREATE TABLE IF NOT EXISTS aggregate_heads (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 18),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS events (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 18),
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
CREATE TABLE IF NOT EXISTS app_principals (
    principal_digest BLOB PRIMARY KEY CHECK (length(principal_digest) = 32),
    principal_kind INTEGER NOT NULL CHECK (principal_kind BETWEEN 1 AND 3),
    actor_id BLOB NOT NULL UNIQUE CHECK (length(actor_id) = 16),
    binding_digest BLOB NOT NULL CHECK (length(binding_digest) = 32),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 2)
) STRICT, WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS app_sessions (
    session_id BLOB PRIMARY KEY CHECK (length(session_id) = 16),
    actor_id BLOB NOT NULL REFERENCES app_principals(actor_id),
    authority_epoch INTEGER NOT NULL CHECK (authority_epoch > 0),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 3),
    created_at INTEGER NOT NULL CHECK (created_at > 0),
    last_protocol_id BLOB NOT NULL CHECK (length(last_protocol_id) = 16),
    last_version_major INTEGER NOT NULL CHECK (last_version_major BETWEEN 1 AND 65535),
    last_version_minor INTEGER NOT NULL CHECK (last_version_minor BETWEEN 0 AND 65535),
    UNIQUE(session_id, actor_id)
) STRICT, WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS app_commands (
    actor_id BLOB NOT NULL CHECK (length(actor_id) = 16),
    session_id BLOB NOT NULL CHECK (length(session_id) = 16),
    idempotency_key BLOB NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 256),
    request_digest BLOB NOT NULL CHECK (length(request_digest) = 32),
    request_id BLOB NOT NULL CHECK (length(request_id) = 16),
    domain_command_digest BLOB NOT NULL CHECK (length(domain_command_digest) = 32),
    command_id BLOB NOT NULL UNIQUE CHECK (length(command_id) = 16),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 4),
    first_position INTEGER REFERENCES events(global_position),
    last_position INTEGER REFERENCES events(global_position),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 128),
    result_digest BLOB CHECK (result_digest IS NULL OR length(result_digest) = 32),
    PRIMARY KEY(actor_id, session_id, idempotency_key),
    FOREIGN KEY(session_id, actor_id) REFERENCES app_sessions(session_id, actor_id),
    CHECK ((state IN (1, 2) AND first_position IS NULL AND last_position IS NULL
            AND error_code IS NULL AND result_digest IS NULL)
        OR (state = 3 AND first_position > 0 AND last_position >= first_position
            AND error_code IS NULL AND result_digest IS NOT NULL)
        OR (state = 4 AND first_position IS NULL AND last_position IS NULL
            AND error_code IS NOT NULL AND result_digest IS NOT NULL))
) STRICT, WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS app_commands_state ON app_commands(state, command_id);
CREATE TABLE IF NOT EXISTS app_prompt_targets (
    prompt_id BLOB PRIMARY KEY CHECK (length(prompt_id) = 16),
    actor_id BLOB NOT NULL CHECK (length(actor_id) = 16),
    session_id BLOB NOT NULL CHECK (length(session_id) = 16),
    originating_request_id BLOB NOT NULL CHECK (length(originating_request_id) = 16),
    target_kind INTEGER NOT NULL CHECK (target_kind BETWEEN 1 AND 2),
    acceptance_spec_id BLOB NOT NULL CHECK (length(acceptance_spec_id) = 16),
    harness_id BLOB NOT NULL CHECK (length(harness_id) = 16),
    workspace_id BLOB NOT NULL CHECK (length(workspace_id) = 16),
    workspace_generation INTEGER NOT NULL CHECK (workspace_generation > 0),
    workspace_revision INTEGER NOT NULL CHECK (workspace_revision > 0),
    policy_id BLOB NOT NULL CHECK (length(policy_id) = 16),
    provider_profile_id BLOB NOT NULL CHECK (length(provider_profile_id) = 16),
    freshness_digest BLOB NOT NULL CHECK (length(freshness_digest) = 32),
    cancellation_generation INTEGER NOT NULL CHECK (cancellation_generation > 0),
    binding_digest BLOB NOT NULL CHECK (length(binding_digest) = 32),
    binding_bytes BLOB NOT NULL CHECK (length(binding_bytes) BETWEEN 1 AND 16777216),
    maximum_answer_bytes INTEGER NOT NULL CHECK (maximum_answer_bytes BETWEEN 1 AND 1048576),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 3),
    settlement_kind INTEGER CHECK (settlement_kind IS NULL OR settlement_kind BETWEEN 1 AND 3),
    settlement_request_id BLOB CHECK (
        settlement_request_id IS NULL OR length(settlement_request_id) = 16
    ),
    settlement_digest BLOB CHECK (settlement_digest IS NULL OR length(settlement_digest) = 32),
    settlement_bytes BLOB CHECK (
        settlement_bytes IS NULL OR length(settlement_bytes) BETWEEN 1 AND 16777216
    ),
    FOREIGN KEY(session_id, actor_id) REFERENCES app_sessions(session_id, actor_id),
    CHECK ((state = 1 AND settlement_kind IS NULL AND settlement_request_id IS NULL
            AND settlement_digest IS NULL AND settlement_bytes IS NULL)
        OR (state = 2 AND settlement_kind IN (1, 2) AND settlement_request_id IS NOT NULL
            AND settlement_digest IS NOT NULL AND settlement_bytes IS NOT NULL)
        OR (state = 3 AND settlement_kind = 3 AND settlement_request_id IS NOT NULL
            AND settlement_digest IS NOT NULL AND settlement_bytes IS NOT NULL)),
    CHECK ((target_kind = 1 AND settlement_kind IN (1, 3))
        OR (target_kind = 2 AND settlement_kind IN (2, 3))
        OR settlement_kind IS NULL)
) STRICT, WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS app_prompt_targets_state ON app_prompt_targets(state, prompt_id);
CREATE TABLE IF NOT EXISTS app_artifacts (
    artifact_id BLOB PRIMARY KEY CHECK (length(artifact_id) = 16),
    digest BLOB NOT NULL UNIQUE CHECK (length(digest) = 32),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    media_type TEXT NOT NULL CHECK (length(media_type) BETWEEN 1 AND 255),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 3),
    producing_position INTEGER REFERENCES events(global_position),
    CHECK ((state IN (1, 3) AND producing_position IS NULL)
        OR (state = 2 AND producing_position IS NOT NULL))
) STRICT, WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS app_workspaces (
    workspace_id BLOB PRIMARY KEY CHECK (length(workspace_id) = 16),
    registration_bytes BLOB NOT NULL CHECK (length(registration_bytes) BETWEEN 1 AND 1048576),
    registration_digest BLOB NOT NULL CHECK (length(registration_digest) = 32),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 3)
) STRICT, WITHOUT ROWID;
COMMIT;
";
