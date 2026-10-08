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
CREATE TABLE IF NOT EXISTS artifact_collection_candidates (
    artifact_digest BLOB PRIMARY KEY NOT NULL CHECK(length(artifact_digest) = 32),
    FOREIGN KEY(artifact_digest) REFERENCES artifact_records(digest) ON DELETE CASCADE
) STRICT;
-- A quarantined row crossed the explicit first collection generation under an older schema.
-- Preserve that owned transition while leaving every legacy active publication protected.
INSERT OR IGNORE INTO artifact_collection_candidates(artifact_digest)
SELECT digest FROM artifact_records WHERE quarantine_state = 2;
CREATE TABLE IF NOT EXISTS artifact_bundles (
    parent_digest BLOB PRIMARY KEY NOT NULL CHECK(length(parent_digest) = 32),
    child_count INTEGER NOT NULL CHECK(child_count >= 0 AND child_count <= 256),
    FOREIGN KEY(parent_digest) REFERENCES artifact_records(digest) ON DELETE CASCADE
) STRICT;
CREATE TABLE IF NOT EXISTS artifact_dependencies (
    parent_digest BLOB NOT NULL CHECK(length(parent_digest) = 32),
    child_index INTEGER NOT NULL CHECK(child_index >= 0 AND child_index < 256),
    child_digest BLOB NOT NULL CHECK(length(child_digest) = 32),
    child_size INTEGER NOT NULL CHECK(child_size >= 0),
    CHECK(parent_digest <> child_digest),
    PRIMARY KEY(parent_digest, child_index),
    UNIQUE(parent_digest, child_digest),
    FOREIGN KEY(parent_digest) REFERENCES artifact_bundles(parent_digest) ON DELETE CASCADE
) STRICT;
CREATE INDEX IF NOT EXISTS artifact_dependencies_child
    ON artifact_dependencies(child_digest, parent_digest);
CREATE TRIGGER IF NOT EXISTS artifact_bundle_parent_is_unbound
BEFORE INSERT ON artifact_bundles
WHEN EXISTS (
        SELECT 1 FROM artifact_references
         WHERE artifact_digest = NEW.parent_digest
    ) OR EXISTS (
        SELECT 1 FROM artifact_dependencies
         WHERE child_digest = NEW.parent_digest
    )
BEGIN
    SELECT RAISE(ABORT, 'artifact bundle parent is already bound');
END;
CREATE TABLE IF NOT EXISTS artifact_operation_sequence (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    last_identity INTEGER NOT NULL CHECK(last_identity >= 0)
) STRICT;
INSERT OR IGNORE INTO artifact_operation_sequence(singleton, last_identity) VALUES (1, 0);
CREATE TABLE IF NOT EXISTS artifact_partial_publications (
    artifact_digest BLOB PRIMARY KEY NOT NULL CHECK(length(artifact_digest) = 32),
    operation_identity INTEGER NOT NULL UNIQUE CHECK(operation_identity > 0),
    FOREIGN KEY(artifact_digest) REFERENCES artifact_records(digest) ON DELETE CASCADE
) STRICT;
CREATE TABLE IF NOT EXISTS artifact_repair_obligations (
    artifact_digest BLOB PRIMARY KEY NOT NULL CHECK(length(artifact_digest) = 32),
    reason INTEGER NOT NULL CHECK(reason IN (1, 2)),
    FOREIGN KEY(artifact_digest) REFERENCES artifact_records(digest) ON DELETE CASCADE
) STRICT;
CREATE TABLE IF NOT EXISTS artifact_repair_containments (
    operation_identity INTEGER PRIMARY KEY NOT NULL CHECK(operation_identity > 0),
    artifact_digest BLOB NOT NULL CHECK(length(artifact_digest) = 32),
    namespace INTEGER NOT NULL CHECK(namespace IN (1, 2, 3)),
    FOREIGN KEY(artifact_digest) REFERENCES artifact_repair_obligations(artifact_digest)
        ON DELETE CASCADE
) STRICT;
CREATE INDEX IF NOT EXISTS artifact_repair_containments_digest
    ON artifact_repair_containments(artifact_digest, operation_identity);
";
