//! Transaction-coherent aggregate history and checkpoint observations.

use rusqlite::Transaction;

use crate::{AggregateKey, CommittedRecord, DurableStateRecord, JournalError, SqliteJournal};

/// An aggregate chain and a requested state row observed in the same read transaction.
///
/// The chain and state bytes are independently integrity-checked. The caller still owns the
/// domain-specific association between them and must validate the checkpoint against replay.
/// This observation is not an authority token for a later write.
#[derive(Debug, Eq, PartialEq)]
pub struct AggregateCheckpointSnapshot {
    records: Vec<CommittedRecord>,
    checkpoint: Option<DurableStateRecord>,
}

impl AggregateCheckpointSnapshot {
    /// Consumes the snapshot without cloning its exact history or checkpoint bytes.
    #[must_use]
    pub fn into_parts(self) -> (Vec<CommittedRecord>, Option<DurableStateRecord>) {
        (self.records, self.checkpoint)
    }
}

impl SqliteJournal {
    /// Reads an aggregate chain and a requested current checkpoint in one snapshot.
    ///
    /// The checkpoint is selected by its namespace and key, not inferred from the aggregate.
    /// Callers must check domain identity, revision and deterministic replay equality themselves.
    /// Absence is retained independently for both components so inconsistencies can be rejected.
    ///
    /// # Errors
    /// Returns a storage, input or integrity error using the same checks as
    /// [`Self::records_for_aggregate`] and [`Self::state_record`].
    pub fn aggregate_checkpoint_snapshot(
        &self,
        aggregate: AggregateKey,
        namespace: u16,
        state_key: &[u8],
    ) -> Result<AggregateCheckpointSnapshot, JournalError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| JournalError::sqlite("begin aggregate checkpoint snapshot", error))?;
        let snapshot = load_checkpoint_snapshot(&transaction, aggregate, namespace, state_key)?;
        transaction.commit().map_err(|error| {
            JournalError::sqlite("complete aggregate checkpoint snapshot", error)
        })?;
        Ok(snapshot)
    }
}

pub(super) fn load_checkpoint_snapshot(
    transaction: &Transaction<'_>,
    aggregate: AggregateKey,
    namespace: u16,
    state_key: &[u8],
) -> Result<AggregateCheckpointSnapshot, JournalError> {
    let records = super::load_snapshot(transaction, aggregate)?;
    let checkpoint = super::super::state::load_state_record(transaction, namespace, state_key)?;
    Ok(AggregateCheckpointSnapshot { records, checkpoint })
}
