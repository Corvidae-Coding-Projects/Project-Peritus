//! Bounded exact-correlation prompt registry.

use std::collections::BTreeMap;

use peritus_app_protocol::{
    PromptAnswer, PromptBinding, PromptCancellation, PromptCorrelation, PromptId, PromptPhase,
    PromptState,
};
use peritus_types::{ActorId, SessionId};

use super::approval::{PreparedAnswer, prepare, validate_binding};
use super::{
    AuthenticatedApprovalResponse, CurrentApprovalAuthority, PromptAcceptance, PromptAdmission,
    PromptBrokerError, PromptBrokerErrorKind, PromptBrokerLimits, PromptCancellationAcceptance,
    PromptTerminalStatus,
};

struct OutstandingPrompt {
    state: PromptState,
    maximum_answer_bytes: usize,
}

/// Bounded owner of awaiting prompts and unretired terminal response facts.
pub struct PromptBroker {
    limits: PromptBrokerLimits,
    entries: BTreeMap<PromptId, OutstandingPrompt>,
}

impl PromptBroker {
    /// Creates an empty broker with a checked positive entry bound.
    #[must_use]
    pub const fn new(limits: PromptBrokerLimits) -> Self {
        Self { limits, entries: BTreeMap::new() }
    }

    /// Registers one exact prompt before it is emitted to a client.
    ///
    /// Approval challenges are canonically decoded immediately and must bind their B1 request
    /// revision and digest to the A3 correlation. Entries remain owned until explicitly retired.
    ///
    /// # Errors
    ///
    /// Rejects capacity exhaustion, duplicate/conflicting prompt identity, invalid answer bounds,
    /// or a malformed approval challenge.
    pub fn register(
        &mut self,
        binding: PromptBinding,
        maximum_answer_bytes: usize,
    ) -> Result<(), PromptBrokerError> {
        let prompt_id = binding.correlation().prompt_id();
        if let Some(existing) = self.entries.get(&prompt_id) {
            let exact = existing.state.binding() == &binding
                && existing.maximum_answer_bytes == maximum_answer_bytes;
            return Err(PromptBrokerError::new(
                if exact {
                    PromptBrokerErrorKind::DuplicateRegistration
                } else {
                    PromptBrokerErrorKind::ConflictingRegistration
                },
                if exact {
                    "the exact prompt is already registered"
                } else {
                    "prompt identity is already bound to different ownership facts"
                },
            ));
        }
        if self.entries.len() >= self.limits.maximum_outstanding() {
            return Err(PromptBrokerError::new(
                PromptBrokerErrorKind::CapacityExceeded,
                "outstanding prompt registry is full",
            ));
        }
        validate_binding(&binding)?;
        let state =
            PromptState::new(binding, maximum_answer_bytes).map_err(PromptBrokerError::protocol)?;
        self.entries.insert(prompt_id, OutstandingPrompt { state, maximum_answer_bytes });
        Ok(())
    }

    /// Admits one answer against authenticated ownership and current freshness observations.
    ///
    /// Signed approval requires [`CurrentApprovalAuthority`]. User input and cancellation never
    /// consume that authority context.
    ///
    /// # Errors
    ///
    /// Rejects unknown, mismatched, stale, cancelled, duplicate, conflicting, constraint-invalid,
    /// or unauthenticated approval input without changing the entry.
    pub fn answer(
        &mut self,
        admission: PromptAdmission,
        answer: PromptAnswer,
        approval_authority: Option<CurrentApprovalAuthority<'_>>,
    ) -> Result<PromptAcceptance, PromptBrokerError> {
        let correlation = answer.correlation();
        let entry = self.entry_mut(correlation.prompt_id())?;
        validate_admission(entry.state.binding().correlation(), correlation, admission)?;
        classify_answer(entry.state.phase(), &answer)?;
        let mut successor = entry.state.clone();
        successor
            .answer(answer.clone(), admission.live_revision())
            .map_err(PromptBrokerError::protocol)?;
        let prepared =
            prepare(entry.state.binding(), answer.payload(), admission, approval_authority)?;
        entry.state = successor;
        Ok(match prepared {
            PreparedAnswer::UserInput => PromptAcceptance::UserInput(answer),
            PreparedAnswer::Cancelled => {
                PromptAcceptance::Cancelled(PromptCancellationAcceptance::ApprovalAnswer(answer))
            }
            PreparedAnswer::Approval { request, signed, observation } => {
                PromptAcceptance::Approval(AuthenticatedApprovalResponse::new(
                    answer,
                    request,
                    signed,
                    observation,
                ))
            }
        })
    }

    /// Admits one dedicated A3 cancellation against current ownership and freshness.
    ///
    /// # Errors
    ///
    /// Rejects unknown, mismatched, stale, duplicate, cancelled, or conflicting input without
    /// changing the entry.
    pub fn cancel(
        &mut self,
        admission: PromptAdmission,
        cancellation: PromptCancellation,
    ) -> Result<PromptCancellationAcceptance, PromptBrokerError> {
        let correlation = cancellation.correlation();
        let entry = self.entry_mut(correlation.prompt_id())?;
        validate_admission(entry.state.binding().correlation(), correlation, admission)?;
        classify_cancellation(entry.state.phase(), cancellation)?;
        entry.state.cancel(cancellation).map_err(PromptBrokerError::protocol)?;
        Ok(PromptCancellationAcceptance::Control(cancellation))
    }

    /// Returns one exact entry's retained lifecycle.
    ///
    /// # Errors
    ///
    /// Rejects an unknown identity or correlation mismatch.
    pub fn status(
        &self,
        correlation: PromptCorrelation,
    ) -> Result<PromptTerminalStatus, PromptBrokerError> {
        let entry = self.entries.get(&correlation.prompt_id()).ok_or_else(not_found)?;
        if entry.state.binding().correlation() != correlation {
            return Err(binding_mismatch());
        }
        Ok(status(entry.state.phase()))
    }

    /// Retires one exact terminal entry after AuthorityOwner has durably handled its result.
    ///
    /// # Errors
    ///
    /// Rejects unknown, mismatched, or still-awaiting prompts.
    pub fn retire_terminal(
        &mut self,
        correlation: PromptCorrelation,
    ) -> Result<PromptTerminalStatus, PromptBrokerError> {
        let terminal = self.status(correlation)?;
        if terminal == PromptTerminalStatus::AwaitingAnswer {
            return Err(PromptBrokerError::new(
                PromptBrokerErrorKind::StillAwaiting,
                "awaiting prompt cannot be retired before cancellation or settlement",
            ));
        }
        self.entries.remove(&correlation.prompt_id());
        Ok(terminal)
    }

    /// Returns all exact correlations owned by one authenticated actor/session pair.
    pub fn correlations_for(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
    ) -> impl Iterator<Item = PromptCorrelation> + '_ {
        self.entries.values().filter_map(move |entry| {
            let correlation = entry.state.binding().correlation();
            (correlation.actor_id() == actor_id && correlation.session_id() == session_id)
                .then_some(correlation)
        })
    }

    /// Returns awaiting and retained-terminal entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no prompt is owned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entry_mut(
        &mut self,
        prompt_id: PromptId,
    ) -> Result<&mut OutstandingPrompt, PromptBrokerError> {
        self.entries.get_mut(&prompt_id).ok_or_else(not_found)
    }
}

fn validate_admission(
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

fn classify_answer(phase: &PromptPhase, answer: &PromptAnswer) -> Result<(), PromptBrokerError> {
    match phase {
        PromptPhase::AwaitingAnswer => Ok(()),
        PromptPhase::Answered(existing) if existing == answer => Err(duplicate_response()),
        PromptPhase::Answered(_) => Err(conflicting_response()),
        PromptPhase::Cancelled(_) => Err(cancelled()),
    }
}

fn classify_cancellation(
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

const fn status(phase: &PromptPhase) -> PromptTerminalStatus {
    match phase {
        PromptPhase::AwaitingAnswer => PromptTerminalStatus::AwaitingAnswer,
        PromptPhase::Answered(_) => PromptTerminalStatus::Answered,
        PromptPhase::Cancelled(_) => PromptTerminalStatus::Cancelled,
    }
}

fn not_found() -> PromptBrokerError {
    PromptBrokerError::new(PromptBrokerErrorKind::NotFound, "prompt identity is not registered")
}

fn binding_mismatch() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::BindingMismatch,
        "prompt response does not echo the complete registered correlation",
    )
}

fn duplicate_response() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::DuplicateResponse,
        "the exact terminal prompt response was already accepted",
    )
}

fn conflicting_response() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::ConflictingResponse,
        "a different terminal prompt response was already accepted",
    )
}

fn cancelled() -> PromptBrokerError {
    PromptBrokerError::new(PromptBrokerErrorKind::Cancelled, "prompt was already cancelled")
}
