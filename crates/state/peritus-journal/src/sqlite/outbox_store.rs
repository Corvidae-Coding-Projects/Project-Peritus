//! Bounded transactional outbox claim and acknowledgement operations.

use crate::{JournalError, JournalErrorKind, OutboxId, OutboxMessage, OutboxState};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::SqliteJournal;

type OutboxClaimRow = (
    Vec<u8>,
    i64,
    String,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<i64>,
    i64,
    i64,
    i64,
    Option<i64>,
);

type OutboxObservationRow = (
    i64,
    String,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<i64>,
    i64,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
);

impl SqliteJournal {
    /// Observes one exact durable outbox row without claiming or changing it.
    ///
    /// The returned row exposes pending, live-wait, expired-reclaim, acknowledged, and accepted
    /// legacy exhaustion through [`OutboxMessage::delivery_status`].
    ///
    /// # Errors
    ///
    /// Returns typed storage or corruption failures for unreadable retained metadata or payload.
    pub fn outbox_message(
        &self,
        id: OutboxId,
    ) -> Result<Option<OutboxMessage>, JournalError> {
        let selected: Option<OutboxObservationRow> = self
            .connection
            .query_row(
                "SELECT producing_position, destination, payload, payload_digest,
                        payload_byte_length, attempts, max_attempts, persistent, state, fence,
                        lease_until
                   FROM outbox WHERE outbox_id = ?1",
                params![id.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| JournalError::sqlite("observe outbox message", error))?;
        let Some((
            position,
            destination,
            inline_payload,
            payload_digest,
            payload_byte_length,
            attempts,
            max_attempts,
            persistent,
            state,
            fence,
            lease_until,
        )) = selected
        else {
            return Ok(None);
        };
        let payload = match (payload_digest, payload_byte_length) {
            (None, None) => {
                if inline_payload.len() > crate::outbox::MAX_OUTBOX_PAYLOAD_BYTES {
                    return Err(super::query::corrupt(
                        "stored outbox payload exceeds its durable bound",
                    ));
                }
                inline_payload
            }
            (Some(digest), byte_length) => super::content::restore_or_inline(
                &self.connection,
                inline_payload,
                super::query::digest_from_blob(&digest, "outbox payload digest")?,
                byte_length,
                crate::outbox::MAX_OUTBOX_PAYLOAD_BYTES,
            )?,
            (None, Some(_)) => {
                return Err(super::query::corrupt(
                    "outbox payload paging metadata is partial",
                ));
            }
        };
        let fence = fence
            .map(|value| super::query::positive_u64(value, "outbox fence"))
            .transpose()?;
        let lease_until = lease_until
            .map(|value| super::query::positive_u64(value, "outbox lease tick"))
            .transpose()?;
        Ok(Some(OutboxMessage {
            id,
            producing_position: super::query::positive_u64(position, "outbox position")?,
            destination,
            payload,
            attempts: u64::try_from(attempts)
                .map_err(|_| super::query::corrupt("stored outbox attempts are invalid"))?,
            max_attempts: u16::try_from(max_attempts).map_err(|_| {
                super::query::corrupt("stored outbox attempt limit is invalid")
            })?,
            persistent: match persistent {
                0 => false,
                1 => true,
                _ => {
                    return Err(super::query::corrupt(
                        "stored outbox delivery policy is invalid",
                    ));
                }
            },
            state: stored_state(state)?,
            fence,
            lease_until,
        }))
    }

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
                "UPDATE outbox SET state = 4, lease_until = NULL
                  WHERE persistent = 0 AND state IN (1, 2) AND attempts >= max_attempts",
                [],
            )
            .map_err(|error| JournalError::sqlite("mark exhausted outbox rows", error))?;
        let selected: Option<OutboxClaimRow> = transaction
            .query_row(
                "SELECT outbox_id, producing_position, destination, payload, payload_digest,
                        payload_byte_length, attempts, max_attempts, persistent, fence
                   FROM outbox
                  WHERE (state = 1 OR (state = 2 AND lease_until <= ?1))
                    AND (persistent = 1 OR attempts < max_attempts)
                  ORDER BY outbox_id LIMIT 1",
                params![now_i64],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| JournalError::sqlite("select outbox claim", error))?;
        let Some((
            id,
            position,
            destination,
            inline_payload,
            payload_digest,
            payload_byte_length,
            attempts,
            max_attempts,
            persistent,
            fence,
        )) = selected
        else {
            transaction
                .commit()
                .map_err(|error| JournalError::sqlite("finish empty outbox claim", error))?;
            return Ok(None);
        };
        let id = OutboxId::new(super::query::array_from_blob(&id, "outbox identity")?)
            .map_err(|_| super::query::corrupt("stored outbox identity is invalid"))?;
        let payload = match (payload_digest, payload_byte_length) {
            (None, None) => {
                if inline_payload.len() > crate::outbox::MAX_OUTBOX_PAYLOAD_BYTES {
                    return Err(super::query::corrupt(
                        "stored outbox payload exceeds its durable bound",
                    ));
                }
                inline_payload
            }
            (Some(digest), Some(byte_length)) => {
                let digest = super::query::digest_from_blob(&digest, "outbox payload digest")?;
                super::content::restore_or_inline(
                    &transaction,
                    inline_payload,
                    digest,
                    Some(byte_length),
                    crate::outbox::MAX_OUTBOX_PAYLOAD_BYTES,
                )?
            }
            _ => return Err(super::query::corrupt("outbox payload paging metadata is partial")),
        };
        let attempts = u64::try_from(attempts)
            .map_err(|_| super::query::corrupt("stored outbox attempts are invalid"))?;
        let max_attempts = u16::try_from(max_attempts)
            .map_err(|_| super::query::corrupt("stored outbox attempt limit is invalid"))?;
        let persistent = match persistent {
            0 => false,
            1 => true,
            _ => return Err(super::query::corrupt("stored outbox delivery policy is invalid")),
        };
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
                    super::append::to_i64(next_attempts, "outbox attempt counter")?,
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
            persistent,
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

fn stored_state(value: i64) -> Result<OutboxState, JournalError> {
    match value {
        1 => Ok(OutboxState::Pending),
        2 => Ok(OutboxState::Claimed),
        3 => Ok(OutboxState::Acknowledged),
        4 => Ok(OutboxState::Exhausted),
        _ => Err(super::query::corrupt("stored outbox state is invalid")),
    }
}
