//! Initial durable artifact catalog schema.

pub const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS artifact_records (
    digest BLOB PRIMARY KEY NOT NULL CHECK(length(digest) = 32),
    size INTEGER NOT NULL CHECK(size >= 0),
    media_type TEXT NOT NULL,
    encryption_algorithm TEXT,
    encryption_key_reference BLOB CHECK(encryption_key_reference IS NULL OR length(encryption_key_reference) = 32),
    encryption_parameters_digest BLOB CHECK(encryption_parameters_digest IS NULL OR length(encryption_parameters_digest) = 32),
    finalization_state INTEGER NOT NULL CHECK(finalization_state IN (1, 2)),
    creating_event BLOB NOT NULL CHECK(length(creating_event) = 16),
    quarantine_state INTEGER NOT NULL CHECK(quarantine_state IN (1, 2)),
    quarantine_generation INTEGER CHECK(quarantine_generation IS NULL OR quarantine_generation > 0),
    integrity_state INTEGER NOT NULL DEFAULT 1 CHECK(integrity_state IN (1, 2)),
    CHECK((quarantine_state = 1 AND quarantine_generation IS NULL)
       OR (quarantine_state = 2 AND quarantine_generation IS NOT NULL))
) STRICT;
CREATE TABLE IF NOT EXISTS artifact_references (
    owner_kind INTEGER NOT NULL CHECK(owner_kind IN (1, 2)),
    owner_identity BLOB NOT NULL CHECK(length(owner_identity) = 32),
    artifact_digest BLOB NOT NULL CHECK(length(artifact_digest) = 32),
    PRIMARY KEY(owner_kind, owner_identity, artifact_digest),
    FOREIGN KEY(artifact_digest) REFERENCES artifact_records(digest) ON DELETE RESTRICT
) STRICT;
CREATE INDEX IF NOT EXISTS artifact_references_digest
    ON artifact_references(artifact_digest);
";
