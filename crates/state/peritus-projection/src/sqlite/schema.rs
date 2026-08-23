//! Stable projection-owned `SQLite` schema.

pub(super) const INSTALL: &str = r"
CREATE TABLE IF NOT EXISTS peritus_projection_generations (
    projection_name TEXT NOT NULL,
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    generation INTEGER NOT NULL CHECK (generation > 0),
    last_position INTEGER NOT NULL CHECK (last_position >= 0),
    journal_head_digest BLOB NOT NULL CHECK (length(journal_head_digest) = 32),
    payload_digest BLOB NOT NULL CHECK (length(payload_digest) = 32),
    schema_digest BLOB NOT NULL CHECK (length(schema_digest) = 32),
    invariant_digest BLOB NOT NULL CHECK (length(invariant_digest) = 32),
    record_count INTEGER NOT NULL CHECK (record_count >= 0),
    payload BLOB NOT NULL,
    PRIMARY KEY (projection_name, projection_version, generation)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_projection_catalog (
    projection_name TEXT NOT NULL,
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    active_generation INTEGER NOT NULL CHECK (active_generation > 0),
    PRIMARY KEY (projection_name, projection_version),
    FOREIGN KEY (projection_name, projection_version, active_generation)
        REFERENCES peritus_projection_generations (
            projection_name, projection_version, generation
        ) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
";
