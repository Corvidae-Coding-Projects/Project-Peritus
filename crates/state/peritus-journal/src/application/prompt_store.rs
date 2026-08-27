//! Transactional application-prompt target and settlement persistence.

use peritus_types::{ActorId, SessionId};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{
    ApplicationPromptId, ApplicationPromptRecord, ApplicationPromptRegistration,
    ApplicationPromptSettlement, ApplicationPromptSettlementKind, ApplicationPromptTargetKind,
    ApplicationSessionState, NewApplicationPromptTarget,
    prompt_rows::PromptRow,
    prompt_types::{settlement_matches, target_matches},
    store::{conflict, invalid, load_session, not_found, to_i64},
};
use crate::{JournalError, SqliteJournal};

const PROMPT_COLUMNS: &str = "prompt_id, actor_id, session_id, originating_request_id, target_kind, \
    acceptance_spec_id, harness_id, workspace_id, workspace_generation, workspace_revision, \
    policy_id, provider_profile_id, freshness_digest, cancellation_generation, binding_digest, \
    binding_bytes, maximum_answer_bytes, state, settlement_kind, settlement_request_id, \
    settlement_digest, settlement_bytes";

impl SqliteJournal {
    /// Durably registers an immutable prompt target before the prompt can be published.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the target session is not active for the actor, or a typed
    /// storage failure.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "registration transfers immutable target ownership"
    )]
    pub fn register_application_prompt_target(
        &mut self,
        target: NewApplicationPromptTarget,
    ) -> Result<ApplicationPromptRegistration, JournalError> {
        let transaction =
            self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(
                |error| JournalError::sqlite("begin application prompt registration", error),
            )?;
        if let Some(existing) = load_prompt(&transaction, target.prompt_id)? {
            let registration = if target_matches(&existing, &target) {
                ApplicationPromptRegistration::Existing(existing)
            } else {
                ApplicationPromptRegistration::Conflict(existing)
            };
            transaction.commit().map_err(|error| {
                JournalError::sqlite("complete application prompt registration", error)
            })?;
            return Ok(registration);
        }
        let session = load_session(&transaction, target.session_id)?
            .ok_or_else(|| invalid("application prompt session does not exist"))?;
        if session.actor_id() != target.actor_id
            || session.state() != ApplicationSessionState::Active
        {
            return Err(invalid("application prompt session is not active for the actor"));
        }
        let revision = target.revision;
        transaction
            .execute(
                "INSERT INTO app_prompt_targets(\
                    prompt_id, actor_id, session_id, originating_request_id, target_kind, \
                    acceptance_spec_id, harness_id, workspace_id, workspace_generation, \
                    workspace_revision, policy_id, provider_profile_id, freshness_digest, \
                    cancellation_generation, binding_digest, binding_bytes, maximum_answer_bytes, \
                    state\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 1)",
                params![
                    target.prompt_id.as_bytes().as_slice(),
                    target.actor_id.as_bytes().as_slice(),
                    target.session_id.as_bytes().as_slice(),
                    target.originating_request_id.as_bytes().as_slice(),
                    target.target_kind.tag(),
                    revision.acceptance_spec_id().as_bytes().as_slice(),
                    revision.harness_id().as_bytes().as_slice(),
                    revision.workspace_id().as_bytes().as_slice(),
                    to_i64(revision.workspace_generation().get(), "prompt workspace generation")?,
                    to_i64(revision.workspace_revision().get(), "prompt workspace revision")?,
                    revision.policy_id().as_bytes().as_slice(),
                    revision.provider_profile_id().as_bytes().as_slice(),
                    target.freshness_digest.as_bytes().as_slice(),
                    to_i64(target.cancellation_generation.get(), "prompt cancellation generation")?,
                    target.binding_digest.as_bytes().as_slice(),
                    target.binding_bytes,
                    i64::try_from(target.maximum_answer_bytes)
                        .map_err(|_| invalid("application prompt answer bound cannot be stored"))?,
                ],
            )
            .map_err(|error| JournalError::sqlite("insert application prompt target", error))?;
        let inserted = load_prompt(&transaction, target.prompt_id)?
            .ok_or_else(|| invalid("inserted application prompt target is not observable"))?;
        transaction.commit().map_err(|error| {
            JournalError::sqlite("commit application prompt registration", error)
        })?;
        Ok(ApplicationPromptRegistration::Inserted(inserted))
    }

    /// Reads one durable prompt target by identity.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity failure.
    pub fn application_prompt_target(
        &self,
        prompt_id: ApplicationPromptId,
    ) -> Result<Option<ApplicationPromptRecord>, JournalError> {
        load_prompt(&self.connection, prompt_id)
    }

    /// Atomically settles one exact prompt target under authenticated ownership and binding.
    ///
    /// Exact replay is idempotent. A different response, owner, or target binding conflicts.
    ///
    /// # Errors
    ///
    /// Returns not found, currentness/ownership conflict, or a typed storage failure.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "settlement atomically transfers exact frame ownership"
    )]
    pub fn settle_application_prompt(
        &mut self,
        prompt_id: ApplicationPromptId,
        actor_id: ActorId,
        session_id: SessionId,
        binding_digest: peritus_types::Sha256Digest,
        settlement: ApplicationPromptSettlement,
    ) -> Result<ApplicationPromptRecord, JournalError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin application prompt settlement", error))?;
        let current = load_prompt(&transaction, prompt_id)?
            .ok_or_else(|| not_found("application prompt target does not exist"))?;
        if current.actor_id() != actor_id
            || current.session_id() != session_id
            || current.binding_digest() != binding_digest
        {
            return Err(conflict("application prompt target binding differs"));
        }
        validate_settlement_kind(current.target_kind(), settlement.kind())?;
        if let Some(existing) = current.settlement() {
            if settlement_matches(existing, &settlement) {
                transaction.commit().map_err(|error| {
                    JournalError::sqlite("complete application prompt settlement replay", error)
                })?;
                return Ok(current);
            }
            return Err(conflict("application prompt already has a different settlement"));
        }
        let state =
            if settlement.kind() == ApplicationPromptSettlementKind::Cancellation { 3 } else { 2 };
        transaction
            .execute(
                "UPDATE app_prompt_targets SET state = ?1, settlement_kind = ?2, \
                    settlement_request_id = ?3, settlement_digest = ?4, settlement_bytes = ?5 \
                 WHERE prompt_id = ?6 AND state = 1",
                params![
                    state,
                    settlement.kind.tag(),
                    settlement.request_id.as_bytes().as_slice(),
                    settlement.digest.as_bytes().as_slice(),
                    settlement.bytes,
                    prompt_id.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| JournalError::sqlite("settle application prompt target", error))?;
        let settled = load_prompt(&transaction, prompt_id)?
            .ok_or_else(|| not_found("settled application prompt target disappeared"))?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("commit application prompt settlement", error))?;
        Ok(settled)
    }
}

fn load_prompt(
    connection: &rusqlite::Connection,
    prompt_id: ApplicationPromptId,
) -> Result<Option<ApplicationPromptRecord>, JournalError> {
    let sql = format!("SELECT {PROMPT_COLUMNS} FROM app_prompt_targets WHERE prompt_id = ?1");
    connection
        .query_row(&sql, params![prompt_id.as_bytes().as_slice()], PromptRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application prompt target", error))?
        .map(PromptRow::parse)
        .transpose()
}

const fn validate_settlement_kind(
    target: ApplicationPromptTargetKind,
    settlement: ApplicationPromptSettlementKind,
) -> Result<(), JournalError> {
    let valid = matches!(
        (target, settlement),
        (ApplicationPromptTargetKind::UserInput, ApplicationPromptSettlementKind::UserInput)
            | (ApplicationPromptTargetKind::Approval, ApplicationPromptSettlementKind::Approval)
            | (_, ApplicationPromptSettlementKind::Cancellation)
    );
    if valid {
        Ok(())
    } else {
        Err(conflict("application prompt settlement kind differs from its target"))
    }
}
