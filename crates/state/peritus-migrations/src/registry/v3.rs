//! Unbounded exact artifact media-type metadata.

pub(super) const SQL: &str = r"CREATE TABLE app_artifacts_v3 (
    artifact_id BLOB PRIMARY KEY CHECK (length(artifact_id) = 16),
    digest BLOB NOT NULL CHECK (length(digest) = 32),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    media_type TEXT NOT NULL CHECK (length(media_type) >= 1),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 3),
    producing_position INTEGER REFERENCES events(global_position),
    CHECK ((state IN (1, 3) AND producing_position IS NULL)
        OR (state = 2 AND producing_position IS NOT NULL))
) STRICT, WITHOUT ROWID;
INSERT INTO app_artifacts_v3 SELECT * FROM app_artifacts;
DROP TABLE app_artifacts;
ALTER TABLE app_artifacts_v3 RENAME TO app_artifacts;
UPDATE store_meta SET schema_version = 3 WHERE singleton = 1;
PRAGMA user_version = 3;
";
pub(super) const DIGEST: [u8; 32] = [
    0xc0, 0x1e, 0x6a, 0x8f, 0x13, 0x92, 0x7b, 0x60, 0xe4, 0xf4, 0x5e, 0x29, 0x48, 0x86, 0xdd, 0x8b,
    0x52, 0x18, 0x92, 0x86, 0xf7, 0x59, 0x86, 0xa8, 0xb8, 0x58, 0x89, 0xc7, 0x9c, 0x3b, 0x99, 0xbd,
];
