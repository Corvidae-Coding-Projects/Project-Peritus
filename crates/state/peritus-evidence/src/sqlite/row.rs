//! Durable row parsing and journal observations.

use crate::admission::DurableJournalObservation;
use crate::{EvidenceError, EvidenceErrorKind, EvidenceRecord, RecoveryAction};
use peritus_artifact_store::ArtifactDigest;
use peritus_types::{EventId, Sha256Digest};
use rusqlite::{OptionalExtension, Transaction};

pub(super) fn journal_observation(
    transaction: &Transaction<'_>,
    position: u64,
) -> Result<DurableJournalObservation, EvidenceError> {
    let position = integer(position, "journal position")?;
    let raw = transaction
        .query_row(
            "SELECT e.global_position, e.event_id, e.event_hash, c.batch_hash, e.frame_family, e.frame_schema, e.frame_digest, e.revision_digest, e.frame FROM events AS e JOIN commands AS c ON c.command_id = e.command_id WHERE e.global_position = ?1 AND e.global_position BETWEEN c.first_position AND c.last_position",
            [position],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?, row.get::<_, i64>(4)?, row.get::<_, i64>(5)?,
                    row.get::<_, Vec<u8>>(6)?, row.get::<_, Vec<u8>>(7)?, row.get::<_, Vec<u8>>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read journal evidence provenance", error))?
        .ok_or_else(|| EvidenceError::new(EvidenceErrorKind::MissingJournalRecord, RecoveryAction::RepairDependency, "read journal evidence provenance", "journal event or owning command is absent"))?;
    let mut statement = transaction
        .prepare("SELECT artifact_digest FROM artifact_references WHERE owner_kind = 1 AND owner_identity = ?1 ORDER BY artifact_digest")
        .map_err(|error| EvidenceError::sqlite("prepare journal artifact references", error))?;
    let artifacts = statement
        .query_map([raw.3.as_slice()], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| EvidenceError::sqlite("query journal artifact references", error))?
        .map(|row| {
            row.map_err(|error| EvidenceError::sqlite("read journal artifact reference", error))
                .and_then(|bytes| digest(&bytes, "journal artifact digest"))
                .map(ArtifactDigest::from_sha256)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DurableJournalObservation {
        global_position: positive(raw.0, "global position")?,
        event_id: EventId::new(array16(&raw.1, "event id")?)
            .map_err(|_| corrupt("event id is reserved"))?,
        event_hash: digest(&raw.2, "event hash")?,
        batch_hash: digest(&raw.3, "batch hash")?,
        frame_family: positive_u16(raw.4, "frame family")?,
        frame_schema: positive_u16(raw.5, "frame schema")?,
        frame_digest: digest(&raw.6, "frame digest")?,
        revision_digest: digest(&raw.7, "revision digest")?,
        frame: raw.8,
        artifacts,
    })
}

pub(super) fn load_record(
    transaction: &Transaction<'_>,
    id: crate::EvidenceId,
) -> Result<Option<EvidenceRecord>, EvidenceError> {
    type EvidenceRow = (Vec<u8>, Vec<u8>, i64, Vec<u8>, Vec<u8>, Vec<u8>);
    let quarantined: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM peritus_evidence_quarantine WHERE evidence_id = ?1)",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| EvidenceError::sqlite("inspect evidence quarantine", error))?;
    if quarantined {
        return Err(corrupt("evidence record is quarantined"));
    }
    let raw: Option<EvidenceRow> = transaction
        .query_row(
            "SELECT record_bytes, record_digest, global_position, event_id, batch_hash, revision_digest FROM peritus_evidence_records WHERE evidence_id = ?1",
            [id.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("load evidence record", error))?;
    raw.map(|(bytes, advertised, position, event, batch, revision)| {
        let record =
            EvidenceRecord::verify_portable(&bytes).map_err(|error| catalog_record(&error))?;
        let provenance = record.provenance();
        if record.id() != id
            || record.record_digest() != digest(&advertised, "record digest")?
            || provenance.global_position() != positive(position, "record position")?
            || provenance.event_id().as_bytes() != &array16(&event, "record event id")?
            || provenance.batch_hash() != digest(&batch, "record batch hash")?
            || provenance.revision_digest() != digest(&revision, "record revision digest")?
        {
            return Err(corrupt("record bytes disagree with indexed columns"));
        }
        validate_record_collections(transaction, &record)?;
        Ok(record)
    })
    .transpose()
}

fn validate_record_collections(
    transaction: &Transaction<'_>,
    record: &EvidenceRecord,
) -> Result<(), EvidenceError> {
    let mut cause_statement = transaction
        .prepare(
            "SELECT parent_id FROM peritus_evidence_causes WHERE child_id = ?1 ORDER BY ordinal",
        )
        .map_err(|error| EvidenceError::sqlite("prepare evidence causes", error))?;
    let causes = cause_statement
        .query_map([record.id().as_bytes().as_slice()], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| EvidenceError::sqlite("query evidence causes", error))?
        .map(|row| {
            let bytes = row.map_err(|error| EvidenceError::sqlite("read evidence cause", error))?;
            crate::EvidenceId::new(array16(&bytes, "cause identity")?)
                .map_err(|_| corrupt("cause identity is reserved"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut artifact_statement = transaction
        .prepare("SELECT artifact_digest FROM peritus_evidence_artifacts WHERE evidence_id = ?1 ORDER BY ordinal")
        .map_err(|error| EvidenceError::sqlite("prepare evidence artifacts", error))?;
    let artifacts = artifact_statement
        .query_map([record.id().as_bytes().as_slice()], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| EvidenceError::sqlite("query evidence artifacts", error))?
        .map(|row| {
            let bytes =
                row.map_err(|error| EvidenceError::sqlite("read evidence artifact", error))?;
            Ok(ArtifactDigest::from_sha256(digest(&bytes, "evidence artifact")?))
        })
        .collect::<Result<Vec<_>, EvidenceError>>()?;
    let mut root_statement = transaction
        .prepare(
            "SELECT artifact_digest FROM artifact_references WHERE owner_kind = 2 AND owner_identity = ?1 ORDER BY artifact_digest",
        )
        .map_err(|error| EvidenceError::sqlite("prepare evidence artifact roots", error))?;
    let roots = root_statement
        .query_map([record.record_digest().as_bytes().as_slice()], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| EvidenceError::sqlite("query evidence artifact roots", error))?
        .map(|row| {
            let bytes =
                row.map_err(|error| EvidenceError::sqlite("read evidence artifact root", error))?;
            Ok(ArtifactDigest::from_sha256(digest(&bytes, "evidence artifact root")?))
        })
        .collect::<Result<Vec<_>, EvidenceError>>()?;
    if causes != record.causes() || artifacts != record.artifacts() || roots != record.artifacts() {
        return Err(corrupt("record bytes disagree with normalized collections"));
    }
    Ok(())
}

pub(super) fn digest(bytes: &[u8], field: &'static str) -> Result<Sha256Digest, EvidenceError> {
    Ok(Sha256Digest::new(array32(bytes, field)?))
}

pub(super) fn integer(value: u64, field: &'static str) -> Result<i64, EvidenceError> {
    i64::try_from(value).map_err(|_| corrupt(&format!("{field} exceeds SQLite INTEGER")))
}

fn positive(value: i64, field: &'static str) -> Result<u64, EvidenceError> {
    let value = u64::try_from(value).map_err(|_| corrupt(&format!("{field} is negative")))?;
    if value == 0 { Err(corrupt(&format!("{field} is zero"))) } else { Ok(value) }
}

fn positive_u16(value: i64, field: &'static str) -> Result<u16, EvidenceError> {
    let value = u16::try_from(value).map_err(|_| corrupt(&format!("{field} is out of range")))?;
    if value == 0 { Err(corrupt(&format!("{field} is zero"))) } else { Ok(value) }
}

fn array16(bytes: &[u8], field: &'static str) -> Result<[u8; 16], EvidenceError> {
    bytes.try_into().map_err(|_| corrupt(&format!("{field} must be 16 bytes")))
}

fn array32(bytes: &[u8], field: &'static str) -> Result<[u8; 32], EvidenceError> {
    bytes.try_into().map_err(|_| corrupt(&format!("{field} must be 32 bytes")))
}

fn corrupt(detail: &str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::CorruptCatalog,
        RecoveryAction::RebuildCatalog,
        "read evidence catalog",
        detail,
    )
}

fn catalog_record(error: &EvidenceError) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::CorruptCatalog,
        RecoveryAction::RebuildCatalog,
        "read evidence catalog",
        error.to_string(),
    )
}
