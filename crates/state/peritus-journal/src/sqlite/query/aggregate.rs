//! Aggregate-head and complete aggregate-chain reads.

use peritus_types::EventSequence;
use rusqlite::{OptionalExtension, params};

use super::{corrupt, digest_from_blob, event_id_from_blob, load_records_range, positive_u64};
use crate::{AggregateHead, AggregateKey, CommittedRecord, JournalError, SqliteJournal};

impl SqliteJournal {
    /// Observes the exact current aggregate head.
    ///
    /// # Errors
    ///
    /// Returns a terminal integrity failure for malformed stored values.
    pub fn head(&self, key: AggregateKey) -> Result<Option<AggregateHead>, JournalError> {
        self.connection
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

    /// Loads one aggregate's checked event chain in sequence order.
    ///
    /// # Errors
    ///
    /// Returns a storage or integrity error for gaps, malformed rows, or hash-chain corruption.
    pub fn records_for_aggregate(
        &self,
        key: AggregateKey,
    ) -> Result<Vec<CommittedRecord>, JournalError> {
        let head = self.head(key)?;
        let Some(head) = head else {
            return Ok(Vec::new());
        };
        let mut statement = self
            .connection
            .prepare(
                "SELECT global_position FROM events
                   WHERE aggregate_kind = ?1 AND aggregate_id = ?2 ORDER BY sequence",
            )
            .map_err(|error| JournalError::sqlite("prepare aggregate replay", error))?;
        let positions = statement
            .query_map(params![key.kind().tag(), key.id().as_bytes().as_slice()], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| JournalError::sqlite("query aggregate replay", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| JournalError::sqlite("read aggregate replay", error))?;
        let mut records = Vec::with_capacity(positions.len());
        for (index, position) in positions.into_iter().enumerate() {
            let position = positive_u64(position, "aggregate event position")?;
            let mut loaded = load_records_range(&self.connection, position, position)?;
            let record = loaded.pop().ok_or_else(|| corrupt("aggregate replay event vanished"))?;
            let expected = u64::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| corrupt("aggregate replay sequence overflowed"))?;
            if record.sequence().get() != expected || record.aggregate() != key {
                return Err(corrupt("aggregate replay sequence is not contiguous"));
            }
            records.push(record);
        }
        if records.last().map(CommittedRecord::event_hash) != Some(head.event_hash()) {
            return Err(corrupt("aggregate replay does not reach its durable head"));
        }
        Ok(records)
    }
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
