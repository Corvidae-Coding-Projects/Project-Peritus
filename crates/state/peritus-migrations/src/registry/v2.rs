//! Independent logical artifact identities sharing content-addressed bytes.

pub(super) const SQL: &str = r"CREATE TABLE app_artifacts_v2 (
    artifact_id BLOB PRIMARY KEY CHECK (length(artifact_id) = 16),
    digest BLOB NOT NULL CHECK (length(digest) = 32),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    media_type TEXT NOT NULL CHECK (length(media_type) BETWEEN 1 AND 255),
    state INTEGER NOT NULL CHECK (state BETWEEN 1 AND 3),
    producing_position INTEGER REFERENCES events(global_position),
    CHECK ((state IN (1, 3) AND producing_position IS NULL)
        OR (state = 2 AND producing_position IS NOT NULL))
) STRICT, WITHOUT ROWID;
INSERT INTO app_artifacts_v2 SELECT * FROM app_artifacts;
DROP TABLE app_artifacts;
ALTER TABLE app_artifacts_v2 RENAME TO app_artifacts;
UPDATE store_meta SET schema_version = 2 WHERE singleton = 1;
PRAGMA user_version = 2;
";
pub(super) const DIGEST: [u8; 32] = [
    0x3a, 0x63, 0xd8, 0x77, 0x5a, 0x14, 0xdf, 0xc7, 0x82, 0x78, 0xa3, 0x50, 0x20, 0x43, 0x60, 0x63,
    0x8b, 0xfa, 0x2f, 0xfb, 0x2f, 0x01, 0xb3, 0x4b, 0x36, 0x1c, 0x5a, 0x53, 0x45, 0xc1, 0x48, 0xb8,
];
