//! Exact schema-version-seven table-copy migration.

pub(super) const SQL: &str = r"PRAGMA defer_foreign_keys = ON;
CREATE TABLE aggregate_heads_v7 (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 14),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
INSERT INTO aggregate_heads_v7(
    aggregate_kind, aggregate_id, sequence, event_id, event_hash
)
SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash
FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id;
CREATE TEMP TABLE migration_v7_head_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v7_head_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM aggregate_heads_v7) FROM aggregate_heads;
DROP TABLE aggregate_heads;
ALTER TABLE aggregate_heads_v7 RENAME TO aggregate_heads;
DROP TABLE migration_v7_head_count;
CREATE TABLE events_v7 (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 14),
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
INSERT INTO events_v7(
    global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
    previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
    revision_digest, causal_ids, frame
)
SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
       previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
       revision_digest, causal_ids, frame
FROM events ORDER BY global_position;
CREATE TEMP TABLE migration_v7_event_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v7_event_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM events_v7) FROM events;
DROP TABLE events;
ALTER TABLE events_v7 RENAME TO events;
CREATE INDEX events_command ON events(command_id, global_position);
DROP TABLE migration_v7_event_count;
UPDATE store_meta SET schema_version = 7 WHERE singleton = 1 AND schema_version = 6;
CREATE TEMP TABLE migration_v7_meta_check(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v7_meta_check(valid)
SELECT COUNT(*) = 1 FROM store_meta WHERE singleton = 1 AND schema_version = 7;
DROP TABLE migration_v7_meta_check;
PRAGMA user_version = 7;
";

// Updated whenever the reviewed exact SQL source changes.
pub(super) const DIGEST: [u8; 32] = [
    0xdb, 0xb7, 0x5a, 0x94, 0x15, 0x62, 0x42, 0x9c, 0x4c, 0x16, 0xdc, 0x1b, 0x03, 0x1d, 0xf5, 0x26,
    0x92, 0xe9, 0x79, 0x6c, 0x23, 0xee, 0x0f, 0xba, 0x02, 0x2c, 0x9c, 0x3f, 0x27, 0x64, 0x04, 0x87,
];
