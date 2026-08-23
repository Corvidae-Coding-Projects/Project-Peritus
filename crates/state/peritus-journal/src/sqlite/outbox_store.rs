//! Bounded transactional outbox claim and acknowledgement operations.

use crate::{JournalError, JournalErrorKind, OutboxId, OutboxMessage, OutboxState};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::SqliteJournal;

type OutboxClaimRow = (Vec<u8>, i64, String, Vec<u8>, i64, i64, Option<i64>);

impl SqliteJournal {
    /// Claims the next pending or expired outbox row under a monotonically increasing fence.
    ///
    /// `lease_until` and `now` are caller-observed positive monotonic ticks; the journal compares
    /// them but does not interpret wall-clock time.
    ///
    /// # Errors
    ///
    /// Rejects zero/non-increasing lease bounds and returns typed storage or overflow failures.
    pub fn claim_outbox(
        &mut self,
        now: u64,
        lease_until: u64,
    ) -> Result<Option<OutboxMessage>, JournalError> {
        if now == 0 || lease_until <= now {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "claim outbox",
                "claim ticks must be positive and strictly increasing",
            ));
        }
        let now_i64 = super::append::to_i64(now, "outbox claim tick")?;
        let lease_i64 = super::append::to_i64(lease_until, "outbox lease tick")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin outbox claim", error))?;
        transaction
            .execute(
                "UPDATE outbox SET state = 4, lease_until = NULL WHERE state IN (1, 2) AND attempts >= max_attempts",
                [],
            )
            .map_err(|error| JournalError::sqlite("mark exhausted outbox rows", error))?;
        let selected: Option<OutboxClaimRow> = transaction
            .query_row(
                "SELECT outbox_id, producing_position, destination, payload, attempts, max_attempts, fence FROM outbox WHERE (state = 1 OR (state = 2 AND lease_until <= ?1)) AND attempts < max_attempts ORDER BY outbox_id LIMIT 1",
                params![now_i64],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("select outbox claim", error))?;
        let Some((id, position, destination, payload, attempts, max_attempts, fence)) = selected
        else {
            transaction
                .commit()
                .map_err(|error| JournalError::sqlite("finish empty outbox claim", error))?;
            return Ok(None);
        };
        let id = OutboxId::new(super::query::array_from_blob(&id, "outbox identity")?)
            .map_err(|_| super::query::corrupt("stored outbox identity is invalid"))?;
        let attempts = u16::try_from(attempts)
            .map_err(|_| super::query::corrupt("stored outbox attempts are invalid"))?;
        let max_attempts = u16::try_from(max_attempts)
            .map_err(|_| super::query::corrupt("stored outbox attempt limit is invalid"))?;
        let next_attempts = attempts.checked_add(1).ok_or_else(|| {
            JournalError::new(
                JournalErrorKind::SequenceOverflow,
                "claim outbox",
                "outbox attempt counter exhausted",
            )
        })?;
        let next_fence = match fence {
            None => 1,
            Some(value) => super::query::positive_u64(value, "outbox fence")?
                .checked_add(1)
                .ok_or_else(|| {
                    JournalError::new(
                        JournalErrorKind::SequenceOverflow,
                        "claim outbox",
                        "outbox fence exhausted",
                    )
                })?,
        };
        transaction
            .execute(
                "UPDATE outbox SET attempts = ?1, state = 2, fence = ?2, lease_until = ?3 WHERE outbox_id = ?4",
                params![
                    i64::from(next_attempts),
                    super::append::to_i64(next_fence, "outbox fence")?,
                    lease_i64,
                    id.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| JournalError::sqlite("persist outbox claim", error))?;
        transaction.commit().map_err(|error| JournalError::sqlite("commit outbox claim", error))?;
        Ok(Some(OutboxMessage {
            id,
            producing_position: super::query::positive_u64(position, "outbox position")?,
            destination,
            payload,
            attempts: next_attempts,
            max_attempts,
            state: OutboxState::Claimed,
            fence: Some(next_fence),
            lease_until: Some(lease_until),
        }))
    }

    /// Idempotently acknowledges a claimed outbox row under its exact fence.
    ///
    /// # Errors
    ///
    /// Returns stale-head for a mismatched fence and not-found for an unknown identity.
    pub fn acknowledge_outbox(&mut self, id: OutboxId, fence: u64) -> Result<(), JournalError> {
        if fence == 0 {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "acknowledge outbox",
                "outbox fence must be positive",
            ));
        }
        let fence = super::append::to_i64(fence, "outbox fence")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin outbox acknowledgement", error))?;
        let affected = transaction
            .execute(
                "UPDATE outbox SET state = 3, lease_until = NULL WHERE outbox_id = ?1 AND state = 2 AND fence = ?2",
                params![id.as_bytes().as_slice(), fence],
            )
            .map_err(|error| JournalError::sqlite("acknowledge outbox", error))?;
        if affected == 1 {
            transaction
                .commit()
                .map_err(|error| JournalError::sqlite("commit outbox acknowledgement", error))?;
            return Ok(());
        }
        let observed = transaction
            .query_row(
                "SELECT state FROM outbox WHERE outbox_id = ?1",
                params![id.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("classify outbox acknowledgement", error))?;
        match observed {
            None => Err(JournalError::new(
                JournalErrorKind::NotFound,
                "acknowledge outbox",
                "outbox identity does not exist",
            )),
            Some(3) => {
                transaction.commit().map_err(|error| {
                    JournalError::sqlite("finish idempotent outbox acknowledgement", error)
                })?;
                Ok(())
            }
            Some(_) => Err(JournalError::new(
                JournalErrorKind::StaleHead,
                "acknowledge outbox",
                "outbox row is not claimed under the supplied fence",
            )),
        }
    }
}
