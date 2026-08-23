//! Idempotent command resolution and committed-batch reconstruction.

use std::collections::BTreeMap;

use peritus_types::{CommandId, Sha256Digest};
use rusqlite::{Connection, OptionalExtension, params};

use super::{corrupt, digest_from_blob, load_records_range, positive_u64};
use crate::{
    AggregateHead, CommandResolution, CommittedBatch, CommittedRecord, JournalError, SqliteJournal,
    hash_chain::batch_hash,
};

type CommandRow = (Vec<u8>, i64, i64, i64, Vec<u8>);

impl SqliteJournal {
    /// Resolves an indeterminate append by the same command identity and request digest.
    ///
    /// # Errors
    ///
    /// Returns storage or terminal integrity failures. A different stored digest is returned as a
    /// bounded conflict observation rather than an error.
    pub fn resolve_command(
        &self,
        command_id: CommandId,
        request_digest: Sha256Digest,
    ) -> Result<CommandResolution, JournalError> {
        resolve_command(&self.connection, self.store_id, command_id, request_digest)
    }
}

pub fn resolve_command(
    connection: &Connection,
    store_id: crate::StoreId,
    command_id: CommandId,
    request_digest: Sha256Digest,
) -> Result<CommandResolution, JournalError> {
    let row: Option<CommandRow> = connection
        .query_row(
            "SELECT request_digest, first_position, last_position, event_count, batch_hash FROM commands WHERE command_id = ?1",
            params![command_id.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()
        .map_err(|error| JournalError::sqlite("resolve command", error))?;
    let Some((stored_digest, first, last, count, stored_batch_hash)) = row else {
        return Ok(CommandResolution::DefinitelyAbsent);
    };
    let stored_digest = digest_from_blob(&stored_digest, "command request digest")?;
    if stored_digest != request_digest {
        return Ok(CommandResolution::Conflict { command_id, stored_digest });
    }
    let first = positive_u64(first, "command first position")?;
    let last = positive_u64(last, "command last position")?;
    let count = positive_u64(count, "command event count")?;
    if last.checked_sub(first).and_then(|span| span.checked_add(1)) != Some(count) {
        return Err(corrupt("command event range is inconsistent"));
    }
    let records = load_records_range(connection, first, last)?;
    if records.len() != usize::try_from(count).map_err(|_| corrupt("command count overflows"))? {
        return Err(corrupt("command range contains missing events"));
    }
    if records.iter().any(|record| record.command_id() != command_id) {
        return Err(corrupt("command range contains another command identity"));
    }
    let stored_batch_hash = digest_from_blob(&stored_batch_hash, "command batch hash")?;
    let artifact_dependencies = load_artifact_dependencies(connection, stored_batch_hash)?;
    let computed_batch_hash = batch_hash(
        store_id,
        command_id,
        request_digest,
        records.iter().map(CommittedRecord::event_hash),
        records.len(),
        artifact_dependencies.iter().map(|dependency| dependency.digest()),
        artifact_dependencies.len(),
    );
    if computed_batch_hash != stored_batch_hash {
        return Err(corrupt("command batch hash does not match its immutable events"));
    }
    let mut final_heads = BTreeMap::new();
    for record in &records {
        final_heads.insert(
            record.aggregate(),
            AggregateHead::new(
                record.aggregate(),
                record.sequence(),
                record.event_id(),
                record.event_hash(),
            ),
        );
    }
    Ok(CommandResolution::Committed(CommittedBatch {
        command_id,
        request_digest,
        first_position: first,
        last_position: last,
        batch_hash: stored_batch_hash,
        records,
        heads: final_heads.into_values().collect(),
        artifact_dependencies,
    }))
}

fn load_artifact_dependencies(
    connection: &Connection,
    batch_hash: Sha256Digest,
) -> Result<Vec<crate::ArtifactDependency>, JournalError> {
    let mut statement = connection
        .prepare(
            "SELECT artifact_digest FROM artifact_references
              WHERE owner_kind = 1 AND owner_identity = ?1 ORDER BY artifact_digest",
        )
        .map_err(|error| JournalError::sqlite("prepare command artifact dependencies", error))?;
    let rows = statement
        .query_map([batch_hash.as_bytes().as_slice()], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| JournalError::sqlite("query command artifact dependencies", error))?;
    rows.map(|row| {
        let bytes = row.map_err(|error| JournalError::sqlite("read artifact dependency", error))?;
        Ok(crate::ArtifactDependency::new(digest_from_blob(&bytes, "artifact dependency digest")?))
    })
    .collect()
}
