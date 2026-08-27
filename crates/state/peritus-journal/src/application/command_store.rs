//! Transactional application-command ledger persistence.

use peritus_types::{CommandId, Sha256Digest};
use rusqlite::{TransactionBehavior, params};

use super::{
    rows::CommandRow,
    store::{
        COMMAND_COLUMNS, conflict, corrupt, invalid, load_command_by_id, load_command_by_key,
        not_found, settlement_matches, to_i64,
    },
    types::{
        ApplicationCommandAdmission, ApplicationCommandRecord, ApplicationCommandSettlement,
        ApplicationCommandState, ApplicationSessionState, NewApplicationCommand, SettlementKind,
    },
};
use crate::{JournalError, SqliteJournal};

impl SqliteJournal {
    /// Atomically classifies and, when new, inserts a pending application command.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the actor/session is not active, or a typed storage error.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "admission consumes the one-shot command input as part of its public contract"
    )]
    pub fn admit_application_command(
        &mut self,
        command: NewApplicationCommand,
    ) -> Result<ApplicationCommandAdmission, JournalError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin application command admission", error))?;
        if let Some(existing) = load_command_by_key(
            &transaction,
            command.actor_id,
            command.session_id,
            &command.idempotency_key,
        )? {
            let admission = if existing.request_digest() == command.request_digest {
                ApplicationCommandAdmission::Existing(existing)
            } else {
                ApplicationCommandAdmission::Conflict(existing)
            };
            transaction.commit().map_err(|error| {
                JournalError::sqlite("complete application command admission", error)
            })?;
            return Ok(admission);
        }
        let session = super::store::load_session(&transaction, command.session_id)?
            .ok_or_else(|| invalid("application command session does not exist"))?;
        if session.actor_id() != command.actor_id
            || session.state() != ApplicationSessionState::Active
        {
            return Err(invalid("application command session is not active for the actor"));
        }
        transaction.execute(
            "INSERT INTO app_commands(actor_id, session_id, idempotency_key, request_digest, request_id, domain_command_digest, command_id, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                command.actor_id.as_bytes().as_slice(), command.session_id.as_bytes().as_slice(),
                command.idempotency_key, command.request_digest.as_bytes().as_slice(),
                command.request_id.as_bytes().as_slice(),
                command.domain_command_digest.as_bytes().as_slice(),
                command.command_id.as_bytes().as_slice(),
            ],
        ).map_err(|error| JournalError::sqlite("insert application command", error))?;
        let inserted = load_command_by_id(&transaction, command.command_id)?
            .ok_or_else(|| corrupt("inserted application command is not observable"))?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("commit application command admission", error))?;
        Ok(ApplicationCommandAdmission::Inserted(inserted))
    }

    /// Reads a command ledger row by its B3 command identity.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity error.
    pub fn application_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<ApplicationCommandRecord>, JournalError> {
        load_command_by_id(&self.connection, command_id)
    }

    /// Applies one checked idempotent command settlement.
    ///
    /// Committed settlements can only be constructed from a real [`crate::CommittedBatch`].
    ///
    /// # Errors
    ///
    /// Returns not found, conflict on digest/terminal drift, or a typed storage error.
    pub fn settle_application_command(
        &mut self,
        command_id: CommandId,
        request_digest: Sha256Digest,
        settlement: ApplicationCommandSettlement,
    ) -> Result<ApplicationCommandRecord, JournalError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin application command settlement", error))?;
        let current = load_command_by_id(&transaction, command_id)?
            .ok_or_else(|| not_found("application command does not exist"))?;
        if current.request_digest() != request_digest {
            return Err(conflict("application command request digest differs"));
        }
        if matches!(
            current.state(),
            ApplicationCommandState::Committed | ApplicationCommandState::Rejected
        ) {
            if settlement_matches(&current, &settlement) {
                transaction.commit().map_err(|error| {
                    JournalError::sqlite("complete application command settlement", error)
                })?;
                return Ok(current);
            }
            return Err(conflict(
                "application command already has a different terminal settlement",
            ));
        }
        match settlement.kind {
            SettlementKind::Indeterminate => {
                transaction
                    .execute(
                        "UPDATE app_commands SET state = 2 WHERE command_id = ?1",
                        params![command_id.as_bytes().as_slice()],
                    )
                    .map_err(|error| {
                        JournalError::sqlite("mark application command indeterminate", error)
                    })?;
            }
            SettlementKind::Committed { first_position, last_position, result_digest } => {
                let event_count: i64 = transaction.query_row(
                    "SELECT COUNT(*) FROM events WHERE global_position BETWEEN ?1 AND ?2 AND command_id = ?3",
                    params![to_i64(first_position, "first committed position")?, to_i64(last_position, "last committed position")?, command_id.as_bytes().as_slice()],
                    |row| row.get(0),
                ).map_err(|error| JournalError::sqlite("validate application committed range", error))?;
                let expected = last_position - first_position + 1;
                if u64::try_from(event_count).ok() != Some(expected) {
                    return Err(conflict(
                        "application settlement does not match the command event range",
                    ));
                }
                transaction.execute(
                    "UPDATE app_commands SET state = 3, first_position = ?1, last_position = ?2, result_digest = ?3 WHERE command_id = ?4",
                    params![to_i64(first_position, "first committed position")?, to_i64(last_position, "last committed position")?, result_digest.as_bytes().as_slice(), command_id.as_bytes().as_slice()],
                ).map_err(|error| JournalError::sqlite("commit application command settlement", error))?;
            }
            SettlementKind::Rejected { error_code, result_digest } => {
                transaction.execute(
                    "UPDATE app_commands SET state = 4, error_code = ?1, result_digest = ?2 WHERE command_id = ?3",
                    params![error_code, result_digest.as_bytes().as_slice(), command_id.as_bytes().as_slice()],
                ).map_err(|error| JournalError::sqlite("reject application command", error))?;
            }
        }
        let settled = load_command_by_id(&transaction, command_id)?
            .ok_or_else(|| corrupt("settled application command disappeared"))?;
        transaction.commit().map_err(|error| {
            JournalError::sqlite("commit application command settlement", error)
        })?;
        Ok(settled)
    }

    /// Reads a bounded deterministic recovery batch of pending and indeterminate commands.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless `max_records` is 1 through 4096, or a storage/integrity error.
    pub fn unsettled_application_commands(
        &self,
        max_records: usize,
    ) -> Result<Vec<ApplicationCommandRecord>, JournalError> {
        if max_records == 0 || max_records > 4_096 {
            return Err(invalid("application command recovery bound is invalid"));
        }
        let sql = format!(
            "SELECT {COMMAND_COLUMNS} FROM app_commands WHERE state IN (1, 2) ORDER BY command_id LIMIT ?1"
        );
        let mut statement = self.connection.prepare(&sql).map_err(|error| {
            JournalError::sqlite("prepare unsettled application commands", error)
        })?;
        let rows = statement
            .query_map(
                [i64::try_from(max_records).map_err(|_| {
                    invalid("application command recovery bound cannot be represented")
                })?],
                CommandRow::read,
            )
            .map_err(|error| JournalError::sqlite("query unsettled application commands", error))?;
        rows.map(|row| {
            row.map_err(|error| JournalError::sqlite("read unsettled application command", error))
                .and_then(CommandRow::parse)
        })
        .collect()
    }
}
