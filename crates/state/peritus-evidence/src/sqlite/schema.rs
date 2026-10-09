//! Stable evidence-owned `SQLite` schema.

pub(super) const INSTALL: &str = r"
CREATE TABLE IF NOT EXISTS peritus_evidence_records (
    evidence_id BLOB PRIMARY KEY NOT NULL CHECK(length(evidence_id) = 16),
    record_digest BLOB NOT NULL UNIQUE CHECK(length(record_digest) = 32),
    global_position INTEGER NOT NULL REFERENCES events(global_position),
    event_id BLOB NOT NULL CHECK(length(event_id) = 16),
    batch_hash BLOB NOT NULL CHECK(length(batch_hash) = 32),
    revision_digest BLOB NOT NULL CHECK(length(revision_digest) = 32),
    record_bytes BLOB NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_evidence_causes (
    child_id BLOB NOT NULL REFERENCES peritus_evidence_records(evidence_id),
    parent_id BLOB NOT NULL REFERENCES peritus_evidence_records(evidence_id),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    PRIMARY KEY(child_id, parent_id),
    UNIQUE(child_id, ordinal),
    CHECK(child_id != parent_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_evidence_artifacts (
    evidence_id BLOB NOT NULL REFERENCES peritus_evidence_records(evidence_id),
    artifact_digest BLOB NOT NULL REFERENCES artifact_records(digest),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    PRIMARY KEY(evidence_id, artifact_digest),
    UNIQUE(evidence_id, ordinal)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_evidence_invalidations (
    target_id BLOB NOT NULL REFERENCES peritus_evidence_records(evidence_id),
    invalidation_digest BLOB NOT NULL UNIQUE CHECK(length(invalidation_digest) = 32),
    global_position INTEGER NOT NULL REFERENCES events(global_position),
    event_id BLOB NOT NULL CHECK(length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK(length(event_hash) = 32),
    reason_digest BLOB NOT NULL CHECK(length(reason_digest) = 32),
    PRIMARY KEY(target_id, invalidation_digest)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_evidence_quarantine (
    evidence_id BLOB PRIMARY KEY NOT NULL CHECK(length(evidence_id) = 16),
    quarantine_digest BLOB NOT NULL UNIQUE CHECK(length(quarantine_digest) = 32),
    record_digest BLOB NOT NULL,
    global_position INTEGER NOT NULL,
    event_id BLOB NOT NULL,
    batch_hash BLOB NOT NULL,
    revision_digest BLOB NOT NULL,
    record_bytes BLOB NOT NULL,
    detected_error TEXT NOT NULL CHECK(length(detected_error) > 0)
) STRICT, WITHOUT ROWID;
";
