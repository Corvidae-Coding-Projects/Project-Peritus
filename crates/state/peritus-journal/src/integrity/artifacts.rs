//! Integrity checks and export rows for committed artifact dependencies.

use rusqlite::Transaction;

use super::{CommittedArtifactReference, corrupt};
use crate::JournalError;

pub(super) fn load_artifact_references(
    transaction: &Transaction<'_>,
) -> Result<Vec<CommittedArtifactReference>, JournalError> {
    let invalid: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artifact_references r
              LEFT JOIN artifact_records a ON a.digest = r.artifact_digest
              WHERE a.digest IS NULL OR a.finalization_state != 2 OR a.quarantine_state != 1)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| JournalError::sqlite("check artifact references", error))?;
    if invalid {
        return Err(corrupt("artifact reference does not name finalized available content"));
    }
    let orphan_owner: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artifact_references r
              LEFT JOIN commands c ON c.batch_hash = r.owner_identity
              WHERE r.owner_kind = 1 AND c.command_id IS NULL)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| JournalError::sqlite("check artifact reference owners", error))?;
    if orphan_owner {
        return Err(corrupt("journal artifact reference has no committed owning batch"));
    }
    let mut statement = transaction
        .prepare(
            "SELECT c.batch_hash, c.first_position, c.last_position, r.artifact_digest
               FROM artifact_references r
               JOIN commands c ON c.batch_hash = r.owner_identity
              WHERE r.owner_kind = 1
              ORDER BY c.first_position, r.artifact_digest",
        )
        .map_err(|error| JournalError::sqlite("prepare artifact reference export", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(|error| JournalError::sqlite("query artifact reference export", error))?;
    rows.map(|row| {
        let (batch_hash, first, last, artifact_digest) =
            row.map_err(|error| JournalError::sqlite("read artifact reference export", error))?;
        Ok(CommittedArtifactReference {
            batch_hash: crate::sqlite::query::digest_from_blob(&batch_hash, "batch hash")?,
            first_position: crate::sqlite::query::positive_u64(
                first,
                "artifact reference first position",
            )?,
            last_position: crate::sqlite::query::positive_u64(
                last,
                "artifact reference last position",
            )?,
            artifact_digest: crate::sqlite::query::digest_from_blob(
                &artifact_digest,
                "artifact digest",
            )?,
        })
    })
    .collect()
}
