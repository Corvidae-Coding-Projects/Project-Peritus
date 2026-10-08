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

CREATE TABLE IF NOT EXISTS peritus_projection_frontiers (
    projection_name TEXT NOT NULL,
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    generation INTEGER NOT NULL CHECK (generation > 0),
    frontier_digest BLOB NOT NULL CHECK (length(frontier_digest) = 32),
    frontier BLOB NOT NULL,
    PRIMARY KEY (projection_name, projection_version, generation),
    FOREIGN KEY (projection_name, projection_version, generation)
        REFERENCES peritus_projection_generations (
            projection_name, projection_version, generation
        ) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_projection_work (
    projection_name TEXT NOT NULL,
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    schema_digest BLOB NOT NULL CHECK (length(schema_digest) = 32),
    owner_store_id BLOB NOT NULL CHECK (length(owner_store_id) = 16),
    source_generation INTEGER CHECK (source_generation > 0),
    phase INTEGER NOT NULL CHECK (phase IN (1, 2)),
    cursor_position INTEGER NOT NULL CHECK (cursor_position >= 0),
    journal_head_digest BLOB CHECK (journal_head_digest IS NULL OR length(journal_head_digest) = 32),
    payload_digest BLOB NOT NULL CHECK (length(payload_digest) = 32),
    invariant_digest BLOB NOT NULL CHECK (length(invariant_digest) = 32),
    frontier_digest BLOB NOT NULL CHECK (length(frontier_digest) = 32),
    work_digest BLOB NOT NULL CHECK (length(work_digest) = 32),
    record_count INTEGER NOT NULL CHECK (record_count >= 0),
    payload BLOB NOT NULL,
    frontier BLOB NOT NULL,
    CHECK ((phase = 1 AND journal_head_digest IS NULL)
        OR (phase = 2 AND journal_head_digest IS NOT NULL)),
    PRIMARY KEY (projection_name, projection_version),
    FOREIGN KEY (projection_name, projection_version, source_generation)
        REFERENCES peritus_projection_generations (
            projection_name, projection_version, generation
        ) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_projection_recovery_roots (
    projection_name TEXT NOT NULL,
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    generation INTEGER NOT NULL CHECK (generation > 0),
    owner_store_id BLOB NOT NULL CHECK (length(owner_store_id) = 16),
    PRIMARY KEY (projection_name, projection_version),
    FOREIGN KEY (projection_name, projection_version, generation)
        REFERENCES peritus_projection_generations (
            projection_name, projection_version, generation
        ) ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_projection_receipts (
    projection_name TEXT NOT NULL,
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    generation INTEGER NOT NULL CHECK (generation > 0),
    owner_store_id BLOB NOT NULL CHECK (length(owner_store_id) = 16),
    last_position INTEGER NOT NULL CHECK (last_position >= 0),
    journal_head_digest BLOB NOT NULL CHECK (length(journal_head_digest) = 32),
    payload_digest BLOB NOT NULL CHECK (length(payload_digest) = 32),
    schema_digest BLOB NOT NULL CHECK (length(schema_digest) = 32),
    invariant_digest BLOB NOT NULL CHECK (length(invariant_digest) = 32),
    frontier_digest BLOB CHECK (frontier_digest IS NULL OR length(frontier_digest) = 32),
    record_count INTEGER NOT NULL CHECK (record_count >= 0),
    PRIMARY KEY (projection_name, projection_version, generation)
) STRICT, WITHOUT ROWID;
";
