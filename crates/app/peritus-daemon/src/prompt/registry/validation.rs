//! Exact prompt response and terminal-state validation.

use peritus_app_protocol::{PromptAnswer, PromptCancellation, PromptCorrelation, PromptPhase};

use crate::prompt::{
    PromptAdmission, PromptBrokerError, PromptBrokerErrorKind, PromptTerminalStatus,
};

pub(super) fn validate_admission(
    expected: PromptCorrelation,
    supplied: PromptCorrelation,
    admission: PromptAdmission,
) -> Result<(), PromptBrokerError> {
    if expected.actor_id() != admission.actor_id() || supplied.actor_id() != admission.actor_id() {
        return Err(PromptBrokerError::new(
            PromptBrokerErrorKind::ActorMismatch,
            "prompt response actor does not match the authenticated peer",
        ));
    }
    if expected.session_id() != admission.session_id()
        || supplied.session_id() != admission.session_id()
    {
        return Err(PromptBrokerError::new(
            PromptBrokerErrorKind::SessionMismatch,
            "prompt response session does not match the authenticated peer",
        ));
    }
    if expected.revision() != admission.live_revision() {
        return Err(PromptBrokerError::new(
            PromptBrokerErrorKind::StaleRevision,
            "prompt revision is no longer current",
        ));
    }
    if expected.cancellation_generation() != admission.cancellation_generation() {
        return Err(PromptBrokerError::new(
            PromptBrokerErrorKind::StaleCancellationGeneration,
            "prompt cancellation generation is no longer current",
        ));
    }
    if supplied != expected {
        return Err(binding_mismatch());
    }
    Ok(())
}

pub(super) fn classify_answer(
    phase: &PromptPhase,
    answer: &PromptAnswer,
) -> Result<(), PromptBrokerError> {
    match phase {
        PromptPhase::AwaitingAnswer => Ok(()),
        PromptPhase::Answered(existing) if existing == answer => Err(duplicate_response()),
        PromptPhase::Answered(_) => Err(conflicting_response()),
        PromptPhase::Cancelled(_) => Err(cancelled()),
    }
}

pub(super) fn classify_cancellation(
    phase: &PromptPhase,
    cancellation: PromptCancellation,
) -> Result<(), PromptBrokerError> {
    match phase {
        PromptPhase::AwaitingAnswer => Ok(()),
        PromptPhase::Answered(_) => Err(conflicting_response()),
        PromptPhase::Cancelled(existing) if *existing == cancellation => Err(duplicate_response()),
        PromptPhase::Cancelled(_) => Err(conflicting_response()),
    }
}

pub(super) const fn status(phase: &PromptPhase) -> PromptTerminalStatus {
    match phase {
        PromptPhase::AwaitingAnswer => PromptTerminalStatus::AwaitingAnswer,
        PromptPhase::Answered(_) => PromptTerminalStatus::Answered,
        PromptPhase::Cancelled(_) => PromptTerminalStatus::Cancelled,
    }
}

pub(super) const fn not_found() -> PromptBrokerError {
    PromptBrokerError::new(PromptBrokerErrorKind::NotFound, "prompt identity is not registered")
}

pub(super) const fn binding_mismatch() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::BindingMismatch,
        "prompt response does not echo the complete registered correlation",
    )
}

const fn duplicate_response() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::DuplicateResponse,
        "the exact terminal prompt response was already accepted",
    )
}

pub(super) const fn conflicting_response() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::ConflictingResponse,
        "a different terminal prompt response was already accepted",
    )
}

pub(super) const fn cancelled() -> PromptBrokerError {
    PromptBrokerError::new(PromptBrokerErrorKind::Cancelled, "prompt was already cancelled")
}
