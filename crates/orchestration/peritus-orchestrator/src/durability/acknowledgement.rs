//! Checked binding between one published E0 directive and its exact claimed C0 outbox row.

use peritus_journal::{OutboxAcknowledgement, OutboxId, SqliteJournal};
use peritus_types::RunId;

use crate::{
    DirectiveId, OrchestratorCommand, OrchestratorCommandKind, OrchestratorError,
    OrchestratorErrorKind, OrchestratorRecoveryAction, OrchestratorTransition,
};

/// Exact claimed C0 acknowledgement for one published E0 directive.
///
/// Construction checks inert command and identity bindings. C0 remains authoritative for whether
/// the row is currently claimed under the supplied fence when the transition commits.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClaimedDirectiveAcknowledgement {
    run_id: RunId,
    directive_id: DirectiveId,
    acknowledgement: OutboxAcknowledgement,
}

impl ClaimedDirectiveAcknowledgement {
    /// Binds a published directive to its exact claimed outbox identity and positive fence.
    ///
    /// # Errors
    ///
    /// Rejects a non-publication command, another outbox identity, or the reserved zero fence.
    pub fn new(
        published_command: &OrchestratorCommand,
        outbox_id: OutboxId,
        fence: u64,
    ) -> Result<Self, OrchestratorError> {
        let OrchestratorCommandKind::PublishDirective { directive } = published_command.kind()
        else {
            return Err(invalid("claimed directive source is not a publication command"));
        };
        if directive.id().as_bytes() != outbox_id.as_bytes() {
            return Err(binding("directive identity differs from claimed outbox identity"));
        }
        let acknowledgement = OutboxAcknowledgement::new(outbox_id, fence)
            .map_err(|_| invalid("claimed directive fence must be positive"))?;
        Ok(Self {
            run_id: published_command.run_id(),
            directive_id: directive.id(),
            acknowledgement,
        })
    }

    /// Returns the E0 run that published the claimed directive.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }

    /// Returns the exact acknowledged E0 directive identity.
    #[must_use]
    pub const fn directive_id(self) -> DirectiveId {
        self.directive_id
    }

    /// Returns the exact claimed C0 outbox identity.
    #[must_use]
    pub const fn outbox_id(self) -> OutboxId {
        self.acknowledgement.id()
    }

    /// Returns the positive C0 claim fence.
    #[must_use]
    pub const fn fence(self) -> u64 {
        self.acknowledgement.fence()
    }
}

/// Atomically commits one E0 `AcknowledgeDirective` transition and settles its claimed C0 row.
///
/// # Errors
///
/// Rejects any command other than the acknowledgement of the exact claimed run and directive.
/// C0 also rejects an absent row, stale fence, conflicting command identity, or failed aggregate
/// compare-and-swap without committing either the E0 transition or the outbox settlement.
pub fn commit_claimed_directive_acknowledgement(
    journal: &mut SqliteJournal,
    claim: ClaimedDirectiveAcknowledgement,
    command: &OrchestratorCommand,
    transition: &OrchestratorTransition,
) -> Result<peritus_journal::CommittedBatch, OrchestratorError> {
    let OrchestratorCommandKind::AcknowledgeDirective { directive_id } = command.kind() else {
        return Err(invalid("claimed directive settlement requires an acknowledgement command"));
    };
    if command.run_id() != claim.run_id || *directive_id != claim.directive_id {
        return Err(binding("acknowledgement command differs from the claimed directive"));
    }
    super::commit_transition(journal, command, transition, Some(claim.acknowledgement))
}

const fn invalid(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::InvalidInput,
        OrchestratorRecoveryAction::CorrectInput,
        detail,
    )
}

const fn binding(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::BindingMismatch,
        OrchestratorRecoveryAction::CorrectInput,
        detail,
    )
}
