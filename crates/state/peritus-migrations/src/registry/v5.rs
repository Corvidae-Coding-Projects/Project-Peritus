//! Exact schema-version-five table-copy migration.

pub(super) const SQL: &str = r"PRAGMA defer_foreign_keys = ON;
CREATE TABLE aggregate_heads_v5 (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 12),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
INSERT INTO aggregate_heads_v5(
    aggregate_kind, aggregate_id, sequence, event_id, event_hash
)
SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash
FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id;
CREATE TEMP TABLE migration_v5_head_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v5_head_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM aggregate_heads_v5) FROM aggregate_heads;
DROP TABLE aggregate_heads;
ALTER TABLE aggregate_heads_v5 RENAME TO aggregate_heads;
DROP TABLE migration_v5_head_count;
CREATE TABLE events_v5 (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 12),
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
INSERT INTO events_v5(
    global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
    previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
    revision_digest, causal_ids, frame
)
SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
       previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
       revision_digest, causal_ids, frame
FROM events ORDER BY global_position;
CREATE TEMP TABLE migration_v5_event_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v5_event_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM events_v5) FROM events;
DROP TABLE events;
ALTER TABLE events_v5 RENAME TO events;
CREATE INDEX events_command ON events(command_id, global_position);
DROP TABLE migration_v5_event_count;
UPDATE store_meta SET schema_version = 5 WHERE singleton = 1 AND schema_version = 4;
CREATE TEMP TABLE migration_v5_meta_check(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v5_meta_check(valid)
SELECT COUNT(*) = 1 FROM store_meta WHERE singleton = 1 AND schema_version = 5;
DROP TABLE migration_v5_meta_check;
PRAGMA user_version = 5;
";

// Updated whenever the reviewed exact SQL source changes.
pub(super) const DIGEST: [u8; 32] = [
    0x3f, 0x77, 0x2d, 0x83, 0xda, 0x56, 0xa5, 0x91, 0xe2, 0xdc, 0xa7, 0x6d, 0xf5, 0xab, 0xa8, 0x0b,
    0x77, 0xc1, 0x65, 0x22, 0x14, 0xac, 0x67, 0x06, 0x0c, 0x4d, 0xff, 0x01, 0x71, 0x5a, 0x77, 0xac,
];
