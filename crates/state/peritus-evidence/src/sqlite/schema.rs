//! Stable evidence-owned `SQLite` schema and atomic recovery migration.

use crate::EvidenceError;
use rusqlite::{Connection, TransactionBehavior};

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
    quarantine_id BLOB PRIMARY KEY NOT NULL CHECK(length(quarantine_id) = 32),
    evidence_id BLOB NOT NULL,
    quarantine_digest BLOB NOT NULL UNIQUE CHECK(length(quarantine_digest) = 32),
    record_digest BLOB NOT NULL,
    global_position INTEGER NOT NULL,
    event_id BLOB NOT NULL,
    batch_hash BLOB NOT NULL,
    revision_digest BLOB NOT NULL,
    record_bytes BLOB NOT NULL,
    detected_error TEXT NOT NULL CHECK(length(detected_error) > 0),
    reconciled_record_digest BLOB CHECK(reconciled_record_digest IS NULL OR length(reconciled_record_digest) = 32),
    reconciliation_digest BLOB CHECK(reconciliation_digest IS NULL OR length(reconciliation_digest) = 32),
    CHECK((reconciled_record_digest IS NULL) = (reconciliation_digest IS NULL))
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS peritus_evidence_containment (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    scan_id INTEGER NOT NULL CHECK(scan_id > 0),
    scanned INTEGER NOT NULL CHECK(scanned >= 0),
    contained INTEGER NOT NULL CHECK(contained >= 0),
    cursor BLOB,
    upper_bound BLOB,
    complete INTEGER NOT NULL CHECK(complete IN (0, 1))
) STRICT, WITHOUT ROWID;
";

pub(super) fn migrate(connection: &mut Connection) -> Result<(), EvidenceError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| EvidenceError::sqlite("begin evidence schema migration", error))?;
    let current: bool = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('peritus_evidence_quarantine')
                 WHERE name = 'quarantine_id'
             )",
            [],
            |row| row.get(0),
        )
        .map_err(|error| EvidenceError::sqlite("inspect evidence quarantine schema", error))?;
    if !current {
        transaction
            .execute_batch(
                "ALTER TABLE peritus_evidence_quarantine
                     RENAME TO peritus_evidence_quarantine_v1;
                 CREATE TABLE peritus_evidence_quarantine (
                    quarantine_id BLOB PRIMARY KEY NOT NULL CHECK(length(quarantine_id) = 32),
                    evidence_id BLOB NOT NULL,
                    quarantine_digest BLOB NOT NULL UNIQUE CHECK(length(quarantine_digest) = 32),
                    record_digest BLOB NOT NULL,
                    global_position INTEGER NOT NULL,
                    event_id BLOB NOT NULL,
                    batch_hash BLOB NOT NULL,
                    revision_digest BLOB NOT NULL,
                    record_bytes BLOB NOT NULL,
                    detected_error TEXT NOT NULL CHECK(length(detected_error) > 0),
                    reconciled_record_digest BLOB CHECK(reconciled_record_digest IS NULL OR length(reconciled_record_digest) = 32),
                    reconciliation_digest BLOB CHECK(reconciliation_digest IS NULL OR length(reconciliation_digest) = 32),
                    CHECK((reconciled_record_digest IS NULL) = (reconciliation_digest IS NULL))
                 ) STRICT, WITHOUT ROWID;
                 INSERT INTO peritus_evidence_quarantine(
                    quarantine_id, evidence_id, quarantine_digest, record_digest,
                    global_position, event_id, batch_hash, revision_digest, record_bytes,
                    detected_error, reconciled_record_digest, reconciliation_digest
                 )
                 SELECT quarantine_digest, evidence_id, quarantine_digest, record_digest,
                        global_position, event_id, batch_hash, revision_digest, record_bytes,
                        detected_error, NULL, NULL
                   FROM peritus_evidence_quarantine_v1;
                 DROP TABLE peritus_evidence_quarantine_v1;",
            )
            .map_err(|error| EvidenceError::sqlite("migrate evidence quarantine schema", error))?;
    }
    transaction
        .execute_batch(
            "CREATE INDEX IF NOT EXISTS peritus_evidence_quarantine_evidence
                 ON peritus_evidence_quarantine(evidence_id);",
        )
        .map_err(|error| EvidenceError::sqlite("index evidence quarantine", error))?;
    transaction
        .commit()
        .map_err(|error| EvidenceError::sqlite("commit evidence schema migration", error))
}
