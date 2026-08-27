//! Immutable event-range loading and exact row validation.

use peritus_types::{CommandId, EventSequence};
use rusqlite::{Connection, params};

use super::{
    array_from_blob, causal_ids_from_blob, corrupt, digest_from_blob, event_id_from_blob,
    positive_u64,
};
use crate::{
    AggregateId, AggregateKey, AggregateKind, CommittedRecord, EventDraft, ExactFrame,
    GlobalEventWindow, JournalError, JournalErrorKind, MAX_GLOBAL_WINDOW_RECORDS, SqliteJournal,
    hash_chain::event_hash,
};

impl SqliteJournal {
    /// Reads at most `max_records` exact records strictly after a global cursor.
    ///
    /// Retention bounds and returned rows are observed in one deferred read transaction. Cursor
    /// zero means origin. A cursor older than [`GlobalEventWindow::earliest`] is not silently
    /// advanced: callers can detect that condition with
    /// [`GlobalEventWindow::has_retention_gap_after`].
    ///
    /// # Errors
    ///
    /// Returns invalid input for a zero or over-limit bound, or a typed storage/integrity error.
    pub fn global_events_after(
        &self,
        cursor: u64,
        max_records: usize,
    ) -> Result<GlobalEventWindow, JournalError> {
        if max_records == 0 || max_records > MAX_GLOBAL_WINDOW_RECORDS {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "query global events",
                "global event query bound is outside the production range",
            ));
        }
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| JournalError::sqlite("begin global event query", error))?;
        let bounds = transaction
            .query_row("SELECT MIN(global_position), MAX(global_position) FROM events", [], |row| {
                Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .map_err(|error| JournalError::sqlite("read global event bounds", error))?;
        let (earliest, latest) = match bounds {
            (Some(earliest), Some(latest)) => (
                positive_u64(earliest, "earliest global event position")?,
                positive_u64(latest, "latest global event position")?,
            ),
            (None, None) => (0, 0),
            _ => return Err(corrupt("global event retention bounds are inconsistent")),
        };
        let records = if latest == 0 || cursor >= latest {
            Vec::new()
        } else {
            let first = cursor.saturating_add(1).max(earliest);
            let last = first
                .saturating_add(u64::try_from(max_records - 1).map_err(|_| {
                    JournalError::new(
                        JournalErrorKind::SequenceOverflow,
                        "query global events",
                        "global event query bound cannot be represented",
                    )
                })?)
                .min(latest);
            let records = load_records_range(&transaction, first, last)?;
            validate_contiguous_window(&records, first, last)?;
            records
        };
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("complete global event query", error))?;
        Ok(GlobalEventWindow::new(earliest, latest, records))
    }
}

fn validate_contiguous_window(
    records: &[CommittedRecord],
    first: u64,
    last: u64,
) -> Result<(), JournalError> {
    let expected = last
        .checked_sub(first)
        .and_then(|distance| distance.checked_add(1))
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| corrupt("global event window length cannot be represented"))?;
    if records.len() != expected
        || records.first().is_none_or(|record| record.global_position() != first)
        || records.last().is_none_or(|record| record.global_position() != last)
        || records
            .windows(2)
            .any(|pair| pair[0].global_position().checked_add(1) != Some(pair[1].global_position()))
    {
        return Err(corrupt("global event positions contain an interior gap"));
    }
    Ok(())
}

pub fn load_records_range(
    connection: &Connection,
    first: u64,
    last: u64,
) -> Result<Vec<CommittedRecord>, JournalError> {
    let mut statement = connection
        .prepare(
            "SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id, previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest, revision_digest, causal_ids, frame FROM events WHERE global_position BETWEEN ?1 AND ?2 ORDER BY global_position",
        )
        .map_err(|error| JournalError::sqlite("prepare event range", error))?;
    let mut rows = statement
        .query(params![
            super::super::append::to_i64(first, "first event position")?,
            super::super::append::to_i64(last, "last event position")?,
        ])
        .map_err(|error| JournalError::sqlite("query event range", error))?;
    let mut records = Vec::new();
    while let Some(row) =
        rows.next().map_err(|error| JournalError::sqlite("read event range", error))?
    {
        let raw = RawRecord {
            global_position: row
                .get(0)
                .map_err(|error| JournalError::sqlite("read event position", error))?,
            event_id: row
                .get(1)
                .map_err(|error| JournalError::sqlite("read event identity", error))?,
            aggregate_kind: row
                .get(2)
                .map_err(|error| JournalError::sqlite("read aggregate kind", error))?,
            aggregate_id: row
                .get(3)
                .map_err(|error| JournalError::sqlite("read aggregate identity", error))?,
            sequence: row
                .get(4)
                .map_err(|error| JournalError::sqlite("read event sequence", error))?,
            previous_event_id: row
                .get(5)
                .map_err(|error| JournalError::sqlite("read predecessor identity", error))?,
            previous_event_hash: row
                .get(6)
                .map_err(|error| JournalError::sqlite("read predecessor hash", error))?,
            event_hash: row
                .get(7)
                .map_err(|error| JournalError::sqlite("read event hash", error))?,
            command_id: row
                .get(8)
                .map_err(|error| JournalError::sqlite("read command identity", error))?,
            frame_family: row
                .get(9)
                .map_err(|error| JournalError::sqlite("read frame family", error))?,
            frame_schema: row
                .get(10)
                .map_err(|error| JournalError::sqlite("read frame schema", error))?,
            frame_digest: row
                .get(11)
                .map_err(|error| JournalError::sqlite("read frame digest", error))?,
            revision_digest: row
                .get(12)
                .map_err(|error| JournalError::sqlite("read revision digest", error))?,
            causal_ids: row
                .get(13)
                .map_err(|error| JournalError::sqlite("read causal identities", error))?,
            frame: row.get(14).map_err(|error| JournalError::sqlite("read exact frame", error))?,
        };
        records.push(parse_record(raw)?);
    }
    Ok(records)
}

struct RawRecord {
    global_position: i64,
    event_id: Vec<u8>,
    aggregate_kind: i64,
    aggregate_id: Vec<u8>,
    sequence: i64,
    previous_event_id: Option<Vec<u8>>,
    previous_event_hash: Vec<u8>,
    event_hash: Vec<u8>,
    command_id: Vec<u8>,
    frame_family: i64,
    frame_schema: i64,
    frame_digest: Vec<u8>,
    revision_digest: Vec<u8>,
    causal_ids: Vec<u8>,
    frame: Vec<u8>,
}

fn parse_record(raw: RawRecord) -> Result<CommittedRecord, JournalError> {
    let global_position = positive_u64(raw.global_position, "global event position")?;
    let event_id = event_id_from_blob(&raw.event_id, "event identity")?;
    let aggregate_kind = AggregateKind::from_tag(raw.aggregate_kind)
        .ok_or_else(|| corrupt("unknown stored aggregate kind"))?;
    let aggregate_id = AggregateId::new(array_from_blob(&raw.aggregate_id, "aggregate identity")?)
        .map_err(|_| corrupt("invalid stored aggregate identity"))?;
    let aggregate = AggregateKey::new(aggregate_kind, aggregate_id);
    let sequence = EventSequence::new(positive_u64(raw.sequence, "event sequence")?)
        .map_err(|_| corrupt("invalid stored event sequence"))?;
    let previous_event_id = raw
        .previous_event_id
        .as_deref()
        .map(|bytes| event_id_from_blob(bytes, "previous event identity"))
        .transpose()?;
    let previous_event_hash = digest_from_blob(&raw.previous_event_hash, "previous event hash")?;
    let stored_event_hash = digest_from_blob(&raw.event_hash, "event hash")?;
    let command_id = CommandId::new(array_from_blob(&raw.command_id, "command identity")?)
        .map_err(|_| corrupt("invalid stored command identity"))?;
    let frame = ExactFrame::new(raw.frame).map_err(|_| corrupt("stored frame is not canonical"))?;
    if i64::from(frame.family()) != raw.frame_family
        || i64::from(frame.schema_version()) != raw.frame_schema
        || frame.digest() != digest_from_blob(&raw.frame_digest, "frame digest")?
    {
        return Err(corrupt("stored frame metadata or digest does not match exact bytes"));
    }
    let revision_digest = digest_from_blob(&raw.revision_digest, "revision digest")?;
    let causal_parents = causal_ids_from_blob(&raw.causal_ids)?;
    let draft = EventDraft::new(
        aggregate,
        sequence,
        event_id,
        previous_event_id,
        frame.clone(),
        revision_digest,
        causal_parents.clone(),
    )
    .map_err(|_| corrupt("stored event fields violate canonical bounds"))?;
    if event_hash(&draft, previous_event_hash, command_id) != stored_event_hash {
        return Err(corrupt("stored event hash does not match exact fields"));
    }
    Ok(CommittedRecord {
        global_position,
        aggregate,
        sequence,
        event_id,
        previous_event_id,
        previous_event_hash,
        event_hash: stored_event_hash,
        command_id,
        frame,
        revision_digest,
        causal_parents,
    })
}
