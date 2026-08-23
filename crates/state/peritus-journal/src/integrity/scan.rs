//! Complete transaction-scoped integrity scan orchestration.

use std::collections::BTreeMap;

use peritus_types::Sha256Digest;
use rusqlite::Transaction;

use super::{
    IntegrityExport, IntegrityReport,
    artifacts::load_artifact_references,
    corrupt,
    validation::{validate_commands, validate_registry, validate_state_records},
};
use crate::{
    AggregateHead, AggregateId, AggregateKey, AggregateKind, CommittedRecord, JournalError, StoreId,
};

pub(super) fn scan_transaction(
    transaction: &Transaction<'_>,
    store_id: StoreId,
) -> Result<IntegrityExport, JournalError> {
    let (count, last): (i64, Option<i64>) = transaction
        .query_row("SELECT COUNT(*), MAX(global_position) FROM events", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|error| JournalError::sqlite("observe integrity range", error))?;
    let event_count = u64::try_from(count).map_err(|_| corrupt("negative event count"))?;
    let last_position = match last {
        Some(value) => crate::sqlite::query::positive_u64(value, "last global position")?,
        None => 0,
    };
    if event_count != last_position {
        return Err(corrupt("global event positions contain a gap or invalid origin"));
    }
    let records = if last_position == 0 {
        Vec::new()
    } else {
        crate::sqlite::query::load_records_range(transaction, 1, last_position)?
    };
    if records.len()
        != usize::try_from(event_count).map_err(|_| corrupt("event count overflows"))?
    {
        return Err(corrupt("event count does not match immutable rows"));
    }
    let expected_heads = validate_event_order(&records)?;
    let stored_heads = load_heads(transaction)?;
    if expected_heads.len() != stored_heads.len()
        || expected_heads.values().zip(&stored_heads).any(|(expected, stored)| expected != stored)
    {
        return Err(corrupt("aggregate head catalog does not match immutable events"));
    }
    validate_commands(transaction, store_id)?;
    validate_state_records(transaction)?;
    validate_registry(transaction)?;
    let artifact_references = load_artifact_references(transaction)?;
    let head_digest = crate::hash_chain::journal_head_hash(
        store_id,
        last_position,
        stored_heads.iter().map(|head| (head.key(), head.sequence().get(), head.event_hash())),
        stored_heads.len(),
    );
    Ok(IntegrityExport {
        report: IntegrityReport {
            store_id,
            event_count,
            aggregate_count: stored_heads.len() as u64,
            last_position,
            journal_head_digest: head_digest,
        },
        records,
        heads: stored_heads,
        artifact_references,
    })
}

fn validate_event_order(
    records: &[CommittedRecord],
) -> Result<BTreeMap<AggregateKey, AggregateHead>, JournalError> {
    let mut heads: BTreeMap<AggregateKey, AggregateHead> = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let expected_position = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| corrupt("global position overflows"))?;
        if record.global_position() != expected_position {
            return Err(corrupt("global positions are not contiguous from one"));
        }
        match heads.get(&record.aggregate()) {
            None => {
                if record.sequence().get() != 1
                    || record.previous_event_id().is_some()
                    || record.previous_event_hash() != Sha256Digest::new([0; 32])
                {
                    return Err(corrupt("aggregate genesis predecessor is invalid"));
                }
            }
            Some(previous) => {
                if previous.sequence().get().checked_add(1) != Some(record.sequence().get())
                    || record.previous_event_id() != Some(previous.event_id())
                    || record.previous_event_hash() != previous.event_hash()
                {
                    return Err(corrupt("aggregate sequence or hash predecessor is broken"));
                }
            }
        }
        heads.insert(
            record.aggregate(),
            AggregateHead::new(
                record.aggregate(),
                record.sequence(),
                record.event_id(),
                record.event_hash(),
            ),
        );
    }
    Ok(heads)
}

fn load_heads(transaction: &Transaction<'_>) -> Result<Vec<AggregateHead>, JournalError> {
    let mut statement = transaction
        .prepare(
            "SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id",
        )
        .map_err(|error| JournalError::sqlite("prepare aggregate heads", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| JournalError::sqlite("query aggregate heads", error))?;
    let mut heads = Vec::new();
    while let Some(row) =
        rows.next().map_err(|error| JournalError::sqlite("read aggregate heads", error))?
    {
        let kind: i64 =
            row.get(0).map_err(|error| JournalError::sqlite("read aggregate kind", error))?;
        let id: Vec<u8> =
            row.get(1).map_err(|error| JournalError::sqlite("read aggregate identity", error))?;
        let sequence: i64 =
            row.get(2).map_err(|error| JournalError::sqlite("read head sequence", error))?;
        let event_id: Vec<u8> =
            row.get(3).map_err(|error| JournalError::sqlite("read head event", error))?;
        let event_hash: Vec<u8> =
            row.get(4).map_err(|error| JournalError::sqlite("read head hash", error))?;
        let kind = AggregateKind::from_tag(kind).ok_or_else(|| corrupt("unknown head kind"))?;
        let id = AggregateId::new(crate::sqlite::query::array_from_blob(&id, "head identity")?)
            .map_err(|_| corrupt("invalid head identity"))?;
        heads.push(crate::sqlite::query::parse_head(
            AggregateKey::new(kind, id),
            sequence,
            &event_id,
            &event_hash,
        )?);
    }
    Ok(heads)
}
