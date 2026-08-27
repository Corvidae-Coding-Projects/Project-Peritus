//! Transactional application-session persistence.

use peritus_types::{ActorId, SessionId};
use rusqlite::{TransactionBehavior, params};

use super::{
    store::{conflict, corrupt, invalid, load_principal_by_actor, load_session, not_found, to_i64},
    types::{
        ApplicationPrincipalState, ApplicationSession, ApplicationSessionState,
        NewApplicationSession,
    },
};
use crate::{JournalError, SqliteJournal};

impl SqliteJournal {
    /// Opens a durable negotiated session for an active bound actor.
    ///
    /// Repeating the exact session facts is idempotent.
    ///
    /// # Errors
    ///
    /// Returns a conflict for identity drift, invalid input for an inactive actor, or storage error.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "opening consumes the one-shot session input as part of its public contract"
    )]
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
}
