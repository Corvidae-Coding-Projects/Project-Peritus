//! Aggregate-head and complete aggregate-chain reads.

mod checkpoint;

pub use checkpoint::AggregateCheckpointSnapshot;

#[cfg(test)]
mod tests;

use peritus_types::EventSequence;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::{corrupt, digest_from_blob, event_id_from_blob, positive_u64};
use crate::{AggregateHead, AggregateKey, CommittedRecord, JournalError, SqliteJournal};

impl SqliteJournal {
    /// Observes the exact current aggregate head.
    ///
    /// # Errors
    ///
    /// Returns a terminal integrity failure for malformed stored values.
    pub fn head(&self, key: AggregateKey) -> Result<Option<AggregateHead>, JournalError> {
        load_head(&self.connection, key)
    }

    /// Loads one aggregate's checked event chain in sequence order.
    ///
    /// The head and all exact rows are read in one snapshot, with one ordered event query
    /// regardless of history length. Every row hash, predecessor, sequence and final head field
    /// is checked; an absent head is valid only when no events exist.
    ///
    /// # Errors
    ///
    /// Returns a storage or integrity error for gaps, malformed rows, or hash-chain corruption.
    pub fn records_for_aggregate(
        &self,
        key: AggregateKey,
    ) -> Result<Vec<CommittedRecord>, JournalError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| JournalError::sqlite("begin aggregate replay", error))?;
        let records = load_snapshot(&transaction, key)?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("complete aggregate replay", error))?;
        Ok(records)
    }
}

fn load_snapshot(
    transaction: &Transaction<'_>,
    key: AggregateKey,
) -> Result<Vec<CommittedRecord>, JournalError> {
    let head = load_head(transaction, key)?;
    let records = super::records::load_aggregate_records(transaction, key)?;
    validate_aggregate_records(key, head, &records)?;
    Ok(records)
}

fn load_head(
    connection: &Connection,
    key: AggregateKey,
) -> Result<Option<AggregateHead>, JournalError> {
    connection
            .query_row(
                "SELECT sequence, event_id, event_hash FROM aggregate_heads WHERE aggregate_kind = ?1 AND aggregate_id = ?2",
                params![key.kind().tag(), key.id().as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| JournalError::sqlite("read aggregate head", error))?
            .map(|(sequence, event_id, event_hash)| {
                parse_head(key, sequence, &event_id, &event_hash)
            })
            .transpose()
}

fn validate_aggregate_records(
    key: AggregateKey,
    head: Option<AggregateHead>,
    records: &[CommittedRecord],
) -> Result<(), JournalError> {
    let mut previous = None;
    for record in records {
        if record.aggregate() != key {
            return Err(corrupt("aggregate replay includes another aggregate"));
        }
        previous = Some(AggregateHead::checked_successor(previous, record)?);
    }
    if previous != head {
        return Err(corrupt("aggregate replay does not reach its exact durable head"));
    }
    Ok(())
}

pub fn parse_head(
    key: AggregateKey,
    sequence: i64,
    event_id: &[u8],
    event_hash: &[u8],
) -> Result<AggregateHead, JournalError> {
    let sequence = EventSequence::new(positive_u64(sequence, "aggregate head sequence")?)
        .map_err(|_| corrupt("invalid aggregate head sequence"))?;
    Ok(AggregateHead::new(
        key,
        sequence,
        event_id_from_blob(event_id, "aggregate head event identity")?,
        digest_from_blob(event_hash, "aggregate head event hash")?,
    ))
}
