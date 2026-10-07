//! Content-addressed bounded rows for exact journal payloads and current state.

pub(super) const SQL: &str = r"CREATE TABLE journal_content (
    content_digest BLOB PRIMARY KEY CHECK (length(content_digest) = 32),
    byte_length INTEGER NOT NULL CHECK (byte_length BETWEEN 0 AND 1073741823),
    chunk_count INTEGER NOT NULL CHECK (chunk_count BETWEEN 0 AND 16384)
) STRICT, WITHOUT ROWID;
CREATE TABLE journal_content_chunks (
    content_digest BLOB NOT NULL REFERENCES journal_content(content_digest),
    chunk_index INTEGER NOT NULL CHECK (chunk_index BETWEEN 0 AND 16383),
    payload BLOB NOT NULL CHECK (length(payload) BETWEEN 1 AND 65536),
    PRIMARY KEY (content_digest, chunk_index)
) STRICT, WITHOUT ROWID;
ALTER TABLE events ADD COLUMN frame_byte_length INTEGER
    CHECK (frame_byte_length BETWEEN 1 AND 1073741823);
ALTER TABLE state_records ADD COLUMN root_digest BLOB
    CHECK (root_digest IS NULL OR length(root_digest) = 32);
ALTER TABLE state_records ADD COLUMN value_bytes INTEGER
    CHECK (value_bytes BETWEEN 0 AND 16777216);
ALTER TABLE outbox ADD COLUMN payload_digest BLOB
    CHECK (payload_digest IS NULL OR length(payload_digest) = 32);
ALTER TABLE outbox ADD COLUMN payload_byte_length INTEGER
    CHECK (payload_byte_length BETWEEN 0 AND 16777216);
ALTER TABLE credential_registry ADD COLUMN snapshot_byte_length INTEGER
    CHECK (snapshot_byte_length BETWEEN 1 AND 1073741823);
ALTER TABLE credential_registry ADD COLUMN snapshot_content_digest BLOB
    CHECK (snapshot_content_digest IS NULL OR length(snapshot_content_digest) = 32);
ALTER TABLE app_commands ADD COLUMN envelope_digest BLOB
    CHECK (envelope_digest IS NULL OR length(envelope_digest) = 32);
ALTER TABLE app_commands ADD COLUMN envelope_byte_length INTEGER
    CHECK (envelope_byte_length BETWEEN 1 AND 1073741823);
ALTER TABLE app_commands ADD COLUMN domain_command_byte_length INTEGER
    CHECK (domain_command_byte_length BETWEEN 1 AND 1073741823);
ALTER TABLE app_prompt_targets ADD COLUMN binding_byte_length INTEGER
    CHECK (binding_byte_length BETWEEN 1 AND 16777216);
ALTER TABLE app_prompt_targets ADD COLUMN settlement_byte_length INTEGER
    CHECK (settlement_byte_length BETWEEN 1 AND 16777216);
ALTER TABLE app_workspaces ADD COLUMN registration_byte_length INTEGER
    CHECK (registration_byte_length BETWEEN 1 AND 1048576);
UPDATE store_meta SET schema_version = 4 WHERE singleton = 1;
PRAGMA user_version = 4;
";
pub(super) const DIGEST: [u8; 32] = [
    0x42, 0x64, 0xd7, 0x92, 0x15, 0xdf, 0xcb, 0x5b, 0x68, 0x0e, 0xad, 0x41, 0xbf, 0x22, 0x53, 0x4c,
    0x90, 0xdf, 0x1a, 0x5d, 0xad, 0x7f, 0xbe, 0x74, 0x1a, 0x4d, 0x1a, 0x96, 0x23, 0x8f, 0x4b, 0x31,
];
