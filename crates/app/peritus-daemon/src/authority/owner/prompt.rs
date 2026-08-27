//! Lifecycle and ownership admission for prompt-broker operations.

use peritus_app_protocol::{PromptBinding, PromptCorrelation};
use peritus_types::{ActorId, SessionId};

use super::error::{require_diagnostic, require_mutation};
use crate::{
    DaemonError, DaemonErrorCode, DaemonLifecycle, DaemonRecovery, PromptTerminalStatus,
    prompt::{PromptBroker, PromptBrokerError, PromptBrokerErrorKind},
};

pub(super) fn register(
    prompts: &mut PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    binding: PromptBinding,
    maximum_answer_bytes: usize,
) -> Result<(), DaemonError> {
    require_mutation(lifecycle)?;
    require_owner(actor_id, session_id, binding.correlation())?;
    prompts.register(binding, maximum_answer_bytes).map_err(prompt_error)
}

pub(super) fn status(
    prompts: &PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    correlation: PromptCorrelation,
) -> Result<PromptTerminalStatus, DaemonError> {
    require_diagnostic(lifecycle)?;
    require_owner(actor_id, session_id, correlation)?;
    prompts.status(correlation).map_err(prompt_error)
}

pub(super) fn retire(
    prompts: &mut PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    correlation: PromptCorrelation,
) -> Result<PromptTerminalStatus, DaemonError> {
    require_mutation(lifecycle)?;
    require_owner(actor_id, session_id, correlation)?;
    prompts.retire_terminal(correlation).map_err(prompt_error)
}

pub(super) fn correlations(
    prompts: &PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    maximum: usize,
) -> Result<Vec<PromptCorrelation>, DaemonError> {
    require_diagnostic(lifecycle)?;
    prompts.correlations_for(actor_id, session_id, maximum).map_err(prompt_error)
}

fn require_owner(
    actor_id: ActorId,
    session_id: SessionId,
    correlation: PromptCorrelation,
) -> Result<(), DaemonError> {
    if correlation.actor_id() != actor_id {
        return Err(ownership_error("prompt correlation belongs to another actor"));
    }
    if correlation.session_id() != session_id {
        return Err(ownership_error("prompt correlation belongs to another session"));
    }
    Ok(())
}

fn prompt_error(error: PromptBrokerError) -> DaemonError {
    let (code, recovery) = match error.kind() {
        PromptBrokerErrorKind::InvalidLimit
        | PromptBrokerErrorKind::ConflictingRegistration
        | PromptBrokerErrorKind::Protocol
        | PromptBrokerErrorKind::ApprovalChallenge
        | PromptBrokerErrorKind::ApprovalAuthorityMissing => {
            (DaemonErrorCode::InvalidInput, DaemonRecovery::CorrectRequest)
        }
        PromptBrokerErrorKind::CapacityExceeded => {
            (DaemonErrorCode::ResourceLimit, DaemonRecovery::Retry)
        }
        PromptBrokerErrorKind::ListingLimitExceeded => {
            (DaemonErrorCode::ResourceLimit, DaemonRecovery::CorrectRequest)
        }
        PromptBrokerErrorKind::ActorMismatch
        | PromptBrokerErrorKind::SessionMismatch
        | PromptBrokerErrorKind::ApprovalAuthentication => {
            (DaemonErrorCode::Unauthorized, DaemonRecovery::CorrectRequest)
        }
        PromptBrokerErrorKind::StillAwaiting => (DaemonErrorCode::NotReady, DaemonRecovery::Retry),
        PromptBrokerErrorKind::DuplicateRegistration
        | PromptBrokerErrorKind::NotFound
        | PromptBrokerErrorKind::BindingMismatch
        | PromptBrokerErrorKind::StaleRevision
        | PromptBrokerErrorKind::StaleCancellationGeneration
        | PromptBrokerErrorKind::Cancelled
        | PromptBrokerErrorKind::DuplicateResponse
        | PromptBrokerErrorKind::ConflictingResponse
        | PromptBrokerErrorKind::StaleCredentialRegistry
        | PromptBrokerErrorKind::StaleAuthorityEpoch => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    DaemonError::with_source(
        code,
        recovery,
        "operate on prompt ownership",
        error.to_string(),
        error,
    )
}

fn ownership_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Unauthorized,
        DaemonRecovery::CorrectRequest,
        "authorize prompt ownership",
        detail,
    )
}
