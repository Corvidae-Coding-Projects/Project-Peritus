//! Atomic checked-plan application.

use crate::{
    AppendPlan, CommandDecision, CommandResolution, JournalError, JournalErrorKind, decide_command,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::SqliteJournal;

impl SqliteJournal {
    /// Atomically applies a deterministic append plan or returns its original committed result.
    ///
    /// # Errors
    ///
    /// Returns typed stale-CAS, idempotency, artifact, storage, or indeterminate-commit failures.
    pub fn append(&mut self, plan: AppendPlan) -> Result<crate::CommittedBatch, JournalError> {
        self.append_inner(plan, false)
    }

    fn append_inner(
        &mut self,
        plan: AppendPlan,
        lose_acknowledgement: bool,
    ) -> Result<crate::CommittedBatch, JournalError> {
        if plan.store_id != self.store_id {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "append batch",
                "append plan names another store",
            ));
        }
        let command_id = plan.command_id;
        let request_digest = plan.request_digest;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin append transaction", error))?;

        let stored_digest: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT request_digest FROM commands WHERE command_id = ?1",
                params![command_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("resolve command before append", error))?;
        let stored_digest = stored_digest
            .map(|bytes| super::query::digest_from_blob(&bytes, "command request digest"))
            .transpose()?;
        match decide_command(stored_digest, request_digest) {
            CommandDecision::Replay => {
                transaction
                    .commit()
                    .map_err(|error| JournalError::sqlite("finish replay observation", error))?;
                return match self.resolve_command(command_id, request_digest)? {
                    CommandResolution::Committed(batch) => Ok(batch),
                    CommandResolution::DefinitelyAbsent | CommandResolution::Conflict { .. } => {
                        Err(JournalError::new(
                            JournalErrorKind::CorruptJournal,
                            "resolve replay",
                            "immutable command changed across observation",
                        ))
                    }
                };
            }
            CommandDecision::Conflict => {
                return Err(JournalError::new(
                    JournalErrorKind::IdempotencyConflict,
                    "append batch",
                    "command identity is bound to another request digest",
                ));
            }
            CommandDecision::New => {}
        }

        super::preconditions::verify_all(&transaction, &plan)?;

        let (first_position, last_position) = super::apply_rows::apply_rows(&transaction, &plan)?;
        transaction
            .execute(
                "INSERT INTO commands(command_id, request_digest, first_position, last_position, event_count, batch_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    command_id.as_bytes().as_slice(),
                    request_digest.as_bytes().as_slice(),
                    to_i64(first_position, "first event position")?,
                    to_i64(last_position, "last event position")?,
                    to_i64(plan.events.len() as u64, "event count")?,
                    plan.batch_hash.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| JournalError::sqlite("record command result", error))?;
        if let Err(commit_error) = transaction.commit() {
            return match self.resolve_command(command_id, request_digest) {
                Ok(CommandResolution::Committed(batch)) => Ok(batch),
                Ok(CommandResolution::DefinitelyAbsent | CommandResolution::Conflict { .. })
                | Err(_) => {
                    Err(JournalError::indeterminate("commit append transaction", commit_error))
                }
            };
        }
        drop(plan);

        if lose_acknowledgement {
            return Err(JournalError::new(
                JournalErrorKind::IndeterminateCommit,
                "acknowledge committed batch",
                "commit completed but acknowledgement was lost",
            ));
        }
        match self.resolve_command(command_id, request_digest)? {
            CommandResolution::Committed(batch) => Ok(batch),
            CommandResolution::DefinitelyAbsent | CommandResolution::Conflict { .. } => {
                Err(JournalError::new(
                    JournalErrorKind::CorruptJournal,
                    "observe committed batch",
                    "post-commit command observation did not match",
                ))
            }
        }
    }

    #[cfg(test)]
    pub(super) fn append_losing_acknowledgement(
        &mut self,
        plan: AppendPlan,
    ) -> Result<crate::CommittedBatch, JournalError> {
        self.append_inner(plan, true)
    }
}

pub(super) fn to_i64(value: u64, operation: &'static str) -> Result<i64, JournalError> {
    i64::try_from(value).map_err(|_| {
        JournalError::new(
            JournalErrorKind::SequenceOverflow,
            operation,
            "value exceeds SQLite signed-integer range",
        )
    })
}
