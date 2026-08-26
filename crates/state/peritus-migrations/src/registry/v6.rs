//! Exact schema-version-six table-copy migration.

pub(super) const SQL: &str = r"PRAGMA defer_foreign_keys = ON;
CREATE TABLE aggregate_heads_v6 (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 13),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
INSERT INTO aggregate_heads_v6(
    aggregate_kind, aggregate_id, sequence, event_id, event_hash
)
SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash
FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id;
CREATE TEMP TABLE migration_v6_head_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v6_head_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM aggregate_heads_v6) FROM aggregate_heads;
DROP TABLE aggregate_heads;
ALTER TABLE aggregate_heads_v6 RENAME TO aggregate_heads;
DROP TABLE migration_v6_head_count;
CREATE TABLE events_v6 (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 13),
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
INSERT INTO events_v6(
    global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
    previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
    revision_digest, causal_ids, frame
)
SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
       previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
       revision_digest, causal_ids, frame
FROM events ORDER BY global_position;
CREATE TEMP TABLE migration_v6_event_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v6_event_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM events_v6) FROM events;
DROP TABLE events;
ALTER TABLE events_v6 RENAME TO events;
CREATE INDEX events_command ON events(command_id, global_position);
DROP TABLE migration_v6_event_count;
UPDATE store_meta SET schema_version = 6 WHERE singleton = 1 AND schema_version = 5;
CREATE TEMP TABLE migration_v6_meta_check(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v6_meta_check(valid)
SELECT COUNT(*) = 1 FROM store_meta WHERE singleton = 1 AND schema_version = 6;
DROP TABLE migration_v6_meta_check;
PRAGMA user_version = 6;
";

// Updated whenever the reviewed exact SQL source changes.
pub(super) const DIGEST: [u8; 32] = [
    0x2c, 0xb3, 0xd8, 0x4f, 0x38, 0xeb, 0x2d, 0x69, 0x4c, 0x03, 0x4d, 0xda, 0xb0, 0x83, 0x3f, 0x60,
    0x8a, 0x42, 0x30, 0x4b, 0xb3, 0x70, 0x4a, 0x58, 0xc9, 0x01, 0xf4, 0x5e, 0xf6, 0x69, 0xf3, 0x3b,
];
