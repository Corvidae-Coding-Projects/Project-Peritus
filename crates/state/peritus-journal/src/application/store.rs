//! Transactional application-ledger operations.

use peritus_types::{ActorId, ArtifactId, CommandId, SessionId, Sha256Digest};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{
    rows::{ArtifactRow, CommandRow, PrincipalRow, SessionRow},
    types::{
        ApplicationArtifact, ApplicationCommandAdmission, ApplicationCommandRecord,
        ApplicationCommandSettlement, ApplicationCommandState, ApplicationPrincipal,
        ApplicationPrincipalState, ApplicationSession, ApplicationSessionState,
        NewApplicationArtifact, NewApplicationCommand, NewApplicationPrincipal,
        NewApplicationSession, SettlementKind,
    },
};
use crate::{JournalError, JournalErrorKind, SqliteJournal};

const PRINCIPAL_COLUMNS: &str = "principal_digest, principal_kind, actor_id, binding_digest, state";
const SESSION_COLUMNS: &str = "session_id, actor_id, authority_epoch, state, created_at, \
    last_protocol_id, last_version_major, last_version_minor";
const COMMAND_COLUMNS: &str = "actor_id, session_id, idempotency_key, request_digest, request_id, \
    domain_command_digest, command_id, state, first_position, last_position, error_code, result_digest";
const ARTIFACT_COLUMNS: &str =
    "artifact_id, digest, byte_size, media_type, state, producing_position";

impl SqliteJournal {
    /// Installs an immutable one-to-one OS-principal to actor binding.
    ///
    /// Repeating the exact binding is idempotent. Rebinding either identity is rejected.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict or storage error.
    pub fn bind_application_principal(
        &mut self,
        principal: NewApplicationPrincipal,
    ) -> Result<ApplicationPrincipal, JournalError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin principal binding", error))?;
        if let Some(existing) = load_principal_by_digest(&transaction, principal.principal_digest)?
        {
            if existing.kind() != principal.kind
                || existing.actor_id() != principal.actor_id
                || existing.binding_digest() != principal.binding_digest
            {
                return Err(conflict("principal digest is already bound to different facts"));
            }
            transaction
                .commit()
                .map_err(|error| JournalError::sqlite("complete principal binding", error))?;
            return Ok(existing);
        }
        if load_principal_by_actor(&transaction, principal.actor_id)?.is_some() {
            return Err(conflict("application actor is already bound to another OS principal"));
        }
        transaction.execute(
            "INSERT INTO app_principals(principal_digest, principal_kind, actor_id, binding_digest, state) VALUES (?1, ?2, ?3, ?4, 1)",
            params![
                principal.principal_digest.as_bytes().as_slice(),
                principal.kind.tag(),
                principal.actor_id.as_bytes().as_slice(),
                principal.binding_digest.as_bytes().as_slice(),
            ],
        ).map_err(|error| JournalError::sqlite("insert principal binding", error))?;
        let installed = load_principal_by_digest(&transaction, principal.principal_digest)?
            .ok_or_else(|| corrupt("inserted principal binding is not observable"))?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("commit principal binding", error))?;
        Ok(installed)
    }

    /// Reads a principal binding by canonical digest.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity error.
    pub fn application_principal(
        &self,
        digest: Sha256Digest,
    ) -> Result<Option<ApplicationPrincipal>, JournalError> {
        load_principal_by_digest(&self.connection, digest)
    }

    /// Revokes an existing principal binding without deleting its identity history.
    ///
    /// # Errors
    ///
    /// Returns not found or a typed storage error.
    pub fn revoke_application_principal(
        &mut self,
        digest: Sha256Digest,
    ) -> Result<ApplicationPrincipal, JournalError> {
        let affected = self
            .connection
            .execute(
                "UPDATE app_principals SET state = 2 WHERE principal_digest = ?1",
                params![digest.as_bytes().as_slice()],
            )
            .map_err(|error| JournalError::sqlite("revoke principal binding", error))?;
        if affected == 0 {
            return Err(not_found("application principal does not exist"));
        }
        load_principal_by_digest(&self.connection, digest)?
            .ok_or_else(|| corrupt("revoked principal binding disappeared"))
    }

    /// Opens a durable negotiated session for an active bound actor.
    ///
    /// Repeating the exact session facts is idempotent.
    ///
    /// # Errors
    ///
    /// Returns a conflict for identity drift, invalid input for an inactive actor, or storage error.
    pub fn open_application_session(
        &mut self,
        session: NewApplicationSession,
    ) -> Result<ApplicationSession, JournalError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin application session", error))?;
        if let Some(existing) = load_session(&transaction, session.session_id)? {
            let exact = existing.actor_id() == session.actor_id
                && existing.authority_epoch() == session.authority_epoch
                && existing.created_at() == session.created_at
                && existing.protocol_id() == &session.protocol_id
                && existing.version_major() == session.version_major
                && existing.version_minor() == session.version_minor;
            if !exact {
                return Err(conflict(
                    "application session identity is already bound to different facts",
                ));
            }
            transaction
                .commit()
                .map_err(|error| JournalError::sqlite("complete application session", error))?;
            return Ok(existing);
        }
        let principal = load_principal_by_actor(&transaction, session.actor_id)?
            .ok_or_else(|| invalid("application session actor has no principal binding"))?;
        if principal.state() != ApplicationPrincipalState::Active {
            return Err(invalid("application session actor principal is revoked"));
        }
        transaction.execute(
            "INSERT INTO app_sessions(session_id, actor_id, authority_epoch, state, created_at, last_protocol_id, last_version_major, last_version_minor) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7)",
            params![
                session.session_id.as_bytes().as_slice(), session.actor_id.as_bytes().as_slice(),
                to_i64(session.authority_epoch, "application authority epoch")?,
                to_i64(session.created_at, "application session creation tick")?,
                session.protocol_id.as_slice(), i64::from(session.version_major),
                i64::from(session.version_minor),
            ],
        ).map_err(|error| JournalError::sqlite("insert application session", error))?;
        let installed = load_session(&transaction, session.session_id)?
            .ok_or_else(|| corrupt("inserted application session is not observable"))?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("commit application session", error))?;
        Ok(installed)
    }

    /// Reads a durable application session.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity error.
    pub fn application_session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<ApplicationSession>, JournalError> {
        load_session(&self.connection, session_id)
    }

    /// Records the last successful protocol negotiation for one active actor-owned session.
    ///
    /// # Errors
    ///
    /// Returns not found, rejects an inactive or differently owned session, or returns storage
    /// failure.
    pub fn observe_application_session_negotiation(
        &mut self,
        session_id: SessionId,
        actor_id: ActorId,
        protocol_id: [u8; 16],
        version_major: u16,
        version_minor: u16,
    ) -> Result<ApplicationSession, JournalError> {
        if protocol_id == [0; 16] || version_major == 0 {
            return Err(invalid("application negotiation contains a zero required value"));
        }
        let current = load_session(&self.connection, session_id)?
            .ok_or_else(|| not_found("application session does not exist"))?;
        if current.actor_id() != actor_id || current.state() != ApplicationSessionState::Active {
            return Err(invalid("application session is not active for the authenticated actor"));
        }
        self.connection.execute(
            "UPDATE app_sessions SET last_protocol_id = ?1, last_version_major = ?2, last_version_minor = ?3 WHERE session_id = ?4 AND actor_id = ?5 AND state = 1",
            params![
                protocol_id.as_slice(), i64::from(version_major), i64::from(version_minor),
                session_id.as_bytes().as_slice(), actor_id.as_bytes().as_slice(),
            ],
        ).map_err(|error| JournalError::sqlite("observe application session negotiation", error))?;
        load_session(&self.connection, session_id)?
            .ok_or_else(|| corrupt("observed application session disappeared"))
    }

    /// Advances a session monotonically from active through draining to closed.
    ///
    /// # Errors
    ///
    /// Returns not found, rejects lifecycle reversal, or returns a storage error.
    pub fn advance_application_session(
        &mut self,
        session_id: SessionId,
        state: ApplicationSessionState,
    ) -> Result<ApplicationSession, JournalError> {
        let current = load_session(&self.connection, session_id)?
            .ok_or_else(|| not_found("application session does not exist"))?;
        if state.tag() < current.state().tag() {
            return Err(invalid("application session lifecycle cannot move backwards"));
        }
        self.connection
            .execute(
                "UPDATE app_sessions SET state = ?1 WHERE session_id = ?2",
                params![state.tag(), session_id.as_bytes().as_slice()],
            )
            .map_err(|error| JournalError::sqlite("advance application session", error))?;
        load_session(&self.connection, session_id)?
            .ok_or_else(|| corrupt("advanced application session disappeared"))
    }

    /// Atomically classifies and, when new, inserts a pending application command.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the actor/session is not active, or a typed storage error.
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
        let session = load_session(&transaction, command.session_id)?
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

    /// Inserts exact pending application artifact metadata.
    ///
    /// Repeating exact metadata is idempotent.
    ///
    /// # Errors
    ///
    /// Returns conflict for identity/digest drift, or a typed storage error.
    pub fn begin_application_artifact(
        &mut self,
        artifact: NewApplicationArtifact,
    ) -> Result<ApplicationArtifact, JournalError> {
        if let Some(existing) = load_artifact(&self.connection, artifact.artifact_id)? {
            if existing.digest() == artifact.digest
                && existing.byte_size() == artifact.byte_size
                && existing.media_type() == artifact.media_type
            {
                return Ok(existing);
            }
            return Err(conflict(
                "application artifact identity is already bound to different metadata",
            ));
        }
        self.connection.execute(
            "INSERT INTO app_artifacts(artifact_id, digest, byte_size, media_type, state) VALUES (?1, ?2, ?3, ?4, 1)",
            params![artifact.artifact_id.as_bytes().as_slice(), artifact.digest.as_bytes().as_slice(), to_i64(artifact.byte_size, "application artifact size")?, artifact.media_type],
        ).map_err(|error| JournalError::sqlite("insert application artifact", error))?;
        load_artifact(&self.connection, artifact.artifact_id)?
            .ok_or_else(|| corrupt("inserted application artifact is not observable"))
    }

    /// Marks finalized artifact metadata available at its exact producing event position.
    ///
    /// # Errors
    ///
    /// Returns not found, conflict, or a typed storage error.
    pub fn complete_application_artifact(
        &mut self,
        artifact_id: ArtifactId,
        producing_position: u64,
    ) -> Result<ApplicationArtifact, JournalError> {
        if producing_position == 0 {
            return Err(invalid("artifact producing position must be positive"));
        }
        let affected = self.connection.execute(
            "UPDATE app_artifacts SET state = 2, producing_position = ?1 WHERE artifact_id = ?2 AND (state = 1 OR (state = 2 AND producing_position = ?1))",
            params![to_i64(producing_position, "artifact producing position")?, artifact_id.as_bytes().as_slice()],
        ).map_err(|error| JournalError::sqlite("complete application artifact", error))?;
        if affected == 0 {
            return Err(conflict(
                "application artifact cannot be completed from its current state",
            ));
        }
        load_artifact(&self.connection, artifact_id)?
            .ok_or_else(|| corrupt("completed application artifact disappeared"))
    }

    /// Reads application artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity error.
    pub fn application_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<Option<ApplicationArtifact>, JournalError> {
        load_artifact(&self.connection, artifact_id)
    }
}

fn load_principal_by_digest(
    connection: &Connection,
    digest: Sha256Digest,
) -> Result<Option<ApplicationPrincipal>, JournalError> {
    let sql = format!("SELECT {PRINCIPAL_COLUMNS} FROM app_principals WHERE principal_digest = ?1");
    connection
        .query_row(&sql, params![digest.as_bytes().as_slice()], PrincipalRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application principal", error))?
        .map(PrincipalRow::parse)
        .transpose()
}

fn load_principal_by_actor(
    connection: &Connection,
    actor: ActorId,
) -> Result<Option<ApplicationPrincipal>, JournalError> {
    let sql = format!("SELECT {PRINCIPAL_COLUMNS} FROM app_principals WHERE actor_id = ?1");
    connection
        .query_row(&sql, params![actor.as_bytes().as_slice()], PrincipalRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application principal actor", error))?
        .map(PrincipalRow::parse)
        .transpose()
}

fn load_session(
    connection: &Connection,
    session: SessionId,
) -> Result<Option<ApplicationSession>, JournalError> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM app_sessions WHERE session_id = ?1");
    connection
        .query_row(&sql, params![session.as_bytes().as_slice()], SessionRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application session", error))?
        .map(SessionRow::parse)
        .transpose()
}

fn load_command_by_key(
    connection: &Connection,
    actor: ActorId,
    session: SessionId,
    key: &[u8],
) -> Result<Option<ApplicationCommandRecord>, JournalError> {
    let sql = format!(
        "SELECT {COMMAND_COLUMNS} FROM app_commands WHERE actor_id = ?1 AND session_id = ?2 AND idempotency_key = ?3"
    );
    connection
        .query_row(
            &sql,
            params![actor.as_bytes().as_slice(), session.as_bytes().as_slice(), key],
            CommandRow::read,
        )
        .optional()
        .map_err(|error| JournalError::sqlite("read application command key", error))?
        .map(CommandRow::parse)
        .transpose()
}

fn load_command_by_id(
    connection: &Connection,
    command: CommandId,
) -> Result<Option<ApplicationCommandRecord>, JournalError> {
    let sql = format!("SELECT {COMMAND_COLUMNS} FROM app_commands WHERE command_id = ?1");
    connection
        .query_row(&sql, params![command.as_bytes().as_slice()], CommandRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application command identity", error))?
        .map(CommandRow::parse)
        .transpose()
}

fn load_artifact(
    connection: &Connection,
    artifact: ArtifactId,
) -> Result<Option<ApplicationArtifact>, JournalError> {
    let sql = format!("SELECT {ARTIFACT_COLUMNS} FROM app_artifacts WHERE artifact_id = ?1");
    connection
        .query_row(&sql, params![artifact.as_bytes().as_slice()], ArtifactRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application artifact", error))?
        .map(ArtifactRow::parse)
        .transpose()
}

fn settlement_matches(
    record: &ApplicationCommandRecord,
    settlement: &ApplicationCommandSettlement,
) -> bool {
    match (&settlement.kind, record.state()) {
        (
            SettlementKind::Committed { first_position, last_position, result_digest },
            ApplicationCommandState::Committed,
        ) => {
            record.first_position() == Some(*first_position)
                && record.last_position() == Some(*last_position)
                && record.result_digest() == Some(*result_digest)
        }
        (
            SettlementKind::Rejected { error_code, result_digest },
            ApplicationCommandState::Rejected,
        ) => {
            record.error_code() == Some(error_code)
                && record.result_digest() == Some(*result_digest)
        }
        _ => false,
    }
}

fn to_i64(value: u64, detail: &'static str) -> Result<i64, JournalError> {
    i64::try_from(value).map_err(|_| {
        JournalError::new(JournalErrorKind::SequenceOverflow, "write application ledger", detail)
    })
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "operate application ledger", detail)
}
const fn conflict(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::IdempotencyConflict, "operate application ledger", detail)
}
const fn not_found(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::NotFound, "operate application ledger", detail)
}
const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "operate application ledger", detail)
}
