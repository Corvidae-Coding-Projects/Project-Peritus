//! Exact precondition-independent approval-use command resolution.

use peritus_approval::ApprovalUseOutcome;
use peritus_types::{CommandId, Sha256Digest};

use crate::{
    CommandResolution, CurrentCredentialRegistry, JournalError, JournalErrorKind, SqliteJournal,
    StateInstall,
};

use super::{
    CommittedApprovalUse, DOMAIN, NAMESPACE, approval_install, input, validate_outcome,
    validate_registry,
};

/// Exact idempotency lookup for an approve-once atomic append.
pub struct ApprovalUseResolutionRequest {
    command_id: CommandId,
    request_digest: Sha256Digest,
    install_key: Vec<u8>,
    install_expected_revision: u64,
    install_revision: u64,
    install_digest: Sha256Digest,
    registry_binding: (u64, u64, Sha256Digest),
}

impl ApprovalUseResolutionRequest {
    /// Reconstructs the final command digest before aggregate-head validation.
    ///
    /// `state_installs` must contain every non-approval state successor from the original atomic
    /// request. The outcome is borrowed so a definitely-absent resolution can leave it available
    /// for the live commit path.
    ///
    /// # Errors
    ///
    /// Rejects an inconsistent outcome, stale registry binding, invalid state installs, or
    /// approval-state revision overflow.
    pub fn new(
        command_id: CommandId,
        base_request_digest: Sha256Digest,
        mut state_installs: Vec<StateInstall>,
        outcome: &ApprovalUseOutcome,
        expected_revision: u64,
        registry: &CurrentCredentialRegistry,
    ) -> Result<Self, JournalError> {
        if expected_revision == 0 {
            return Err(input("approval use requires a positive durable predecessor revision"));
        }
        validate_outcome(outcome.aggregate(), outcome.transition(), outcome.consumed())?;
        let registry_binding =
            validate_registry(outcome.aggregate(), outcome.transition(), registry)?;
        let install = approval_install(outcome, expected_revision, registry_binding)?;
        let install_key = install.key().to_vec();
        let install_revision = install.revision();
        let install_digest = install.digest();
        state_installs.push(install);
        let registry_digest = crate::append_plan::bind_registry_current_digest(
            base_request_digest,
            registry_binding.0,
            registry_binding.1,
            registry_binding.2,
        );
        let request_digest = crate::append_plan::bind_domain_state_digest(
            registry_digest,
            DOMAIN,
            &mut state_installs,
        )?;
        Ok(Self {
            command_id,
            request_digest,
            install_key,
            install_expected_revision: expected_revision,
            install_revision,
            install_digest,
            registry_binding,
        })
    }
}

/// Result of resolving one exact approve-once command before mutable-head validation.
pub enum ApprovalUseResolution {
    /// The exact composite command was already committed.
    Committed(Box<CommittedApprovalUse>),
    /// No command row exists; returns the move-only outcome for the live commit path.
    DefinitelyAbsent(Box<ApprovalUseOutcome>),
}

impl SqliteJournal {
    /// Resolves an exact approve-once composite command without consulting mutable heads.
    ///
    /// # Errors
    ///
    /// Returns an idempotency conflict, stale-registry error, inconsistent-outcome error, or
    /// terminal integrity failure when the stored receipt does not match its exact state install.
    pub fn resolve_approval_use(
        &self,
        request: &ApprovalUseResolutionRequest,
        outcome: ApprovalUseOutcome,
    ) -> Result<ApprovalUseResolution, JournalError> {
        validate_outcome(outcome.aggregate(), outcome.transition(), outcome.consumed())?;
        let expected_install = approval_install(
            &outcome,
            request.install_expected_revision,
            request.registry_binding,
        )?;
        if expected_install.key() != request.install_key
            || expected_install.revision() != request.install_revision
            || expected_install.digest() != request.install_digest
        {
            return Err(input("approval-use outcome differs from the resolution request"));
        }
        let batch = match self.resolve_command(request.command_id, request.request_digest)? {
            CommandResolution::DefinitelyAbsent => {
                return Ok(ApprovalUseResolution::DefinitelyAbsent(Box::new(outcome)));
            }
            CommandResolution::Conflict { .. } => {
                return Err(JournalError::new(
                    JournalErrorKind::IdempotencyConflict,
                    "resolve approval use",
                    "command identity is bound to a different request digest",
                ));
            }
            CommandResolution::Committed(batch) => batch,
        };
        let state = self
            .state_record_revision(NAMESPACE, &request.install_key, request.install_revision)?
            .ok_or_else(|| {
                JournalError::new(
                    JournalErrorKind::CorruptJournal,
                    "resolve approval use",
                    "committed approval state row is missing",
                )
            })?;
        if state.digest() != request.install_digest
            || state.producing_position() != batch.last_position()
        {
            return Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "resolve approval use",
                "approval state row does not match the exact committed command",
            ));
        }
        let (aggregate, transition, consumed) = outcome.into_parts();
        Ok(ApprovalUseResolution::Committed(Box::new(CommittedApprovalUse {
            batch,
            aggregate,
            transition,
            consumed,
            state_revision: state.revision(),
            state_digest: state.digest(),
            registry_binding: request.registry_binding,
        })))
    }
}
