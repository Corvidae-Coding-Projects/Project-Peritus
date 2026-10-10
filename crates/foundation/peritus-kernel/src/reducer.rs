//! Total authoritative lifecycle reducer.

mod acceptance;
mod action;
mod attempt;
mod refinement;
mod review;
mod run;
mod session;
mod turn;
mod waiver;
mod waiver_evidence;

use crate::{
    AcceptanceOutcome, CommandEnvelope, KernelAggregate, KernelCommand, KernelError,
    KernelErrorKind, KernelEvent, KernelEventKind, KernelOutcome, KernelSubject, KernelTransition,
    ReducerInputs,
};
use peritus_types::EventSequence;

struct AppliedCommand {
    event_kind: KernelEventKind,
    subject: KernelSubject,
    acceptance_outcome: Option<AcceptanceOutcome>,
}
impl AppliedCommand {
    const fn new(event_kind: KernelEventKind, subject: KernelSubject) -> Self {
        Self { event_kind, subject, acceptance_outcome: None }
    }
    const fn acceptance(
        event_kind: KernelEventKind,
        subject: KernelSubject,
        outcome: AcceptanceOutcome,
    ) -> Self {
        Self { event_kind, subject, acceptance_outcome: Some(outcome) }
    }
}
impl KernelAggregate {
    #[doc = " Applies one command against the exact current causal head."]
    #[doc = ""]
    #[doc = " Every rejection returns this owned aggregate unchanged. Every accepted command returns one"]
    #[doc = " next aggregate and exactly one event plan."]
    #[must_use]
    #[allow(
        clippy::needless_pass_by_value,
        clippy::too_many_lines,
        reason = "the authoritative reducer keeps its causal assembly and direct refinement proof together"
    )]
    pub fn reduce(
        self,
        envelope: CommandEnvelope,
        command: KernelCommand,
        inputs: ReducerInputs<'_>,
    ) -> KernelOutcome {
        let next_sequence = match preflight(&self, envelope, &inputs) {
            Ok(sequence) => sequence,
            Err(error) => return KernelOutcome::Rejected { aggregate: self, error },
        };
        let original = self;
        let mut next = original.clone();
        let previous_event_id = original.head_event_id;
        let applied = match apply_command(&mut next, &command, &inputs) {
            Ok(applied) => applied,
            Err(error) => return KernelOutcome::Rejected { aggregate: original, error },
        };
        let event = KernelEvent::new(
            envelope.event_id,
            envelope.command_id,
            next_sequence,
            Some(previous_event_id),
            original.revision,
            applied.event_kind,
            applied.subject,
        );
        next.head_event_id = envelope.event_id;
        next.last_sequence = next_sequence;
        next.accepted_command_ids.push(envelope.command_id);
        next.event_ids.push(envelope.event_id);
        if !refinement::critical_step_is_legal(
            &original,
            &next,
            applied.event_kind,
            applied.subject,
        ) {
            return KernelOutcome::Rejected {
                aggregate: original,
                error: KernelError::new(KernelErrorKind::InvalidAggregate),
            };
        }
        match applied.event_kind {
            KernelEventKind::AcceptanceAccepted => {}
            _ => {
                if !refinement::no_new_acceptance(&original, &next) {
                    return KernelOutcome::Rejected {
                        aggregate: original,
                        error: KernelError::new(KernelErrorKind::InvalidAggregate),
                    };
                }
            }
        }
        if !next.is_valid() {
            return KernelOutcome::Rejected {
                aggregate: original,
                error: KernelError::new(KernelErrorKind::InvalidAggregate),
            };
        }
        next.revision = original.revision;
        let transition = KernelTransition::new(next, event, applied.acceptance_outcome);
        KernelOutcome::Applied(transition)
    }
}
#[allow(
    clippy::option_if_let_else,
    reason = "explicit result branches remain inside the Verus execution subset"
)]
fn preflight(
    state: &KernelAggregate,
    envelope: CommandEnvelope,
    inputs: &ReducerInputs<'_>,
) -> Result<EventSequence, KernelError> {
    if !state.is_valid() {
        return Err(KernelError::new(KernelErrorKind::InvalidAggregate));
    }
    if !crate::identity::revision_equal(envelope.revision, state.revision) {
        return Err(KernelError::new(KernelErrorKind::RevisionMismatch));
    }
    if !crate::identity::optional_event_id_equal(
        envelope.expected_previous_event_id,
        Some(state.head_event_id),
    ) {
        return Err(KernelError::new(KernelErrorKind::CausalHeadMismatch));
    }
    if inputs.contract().id() != state.contract_binding.contract_id()
        || inputs.contract().content_digest() != state.contract_binding.contract_digest()
        || inputs.contract().bind(state.revision).is_err()
    {
        return Err(KernelError::new(KernelErrorKind::ContractMismatch));
    }
    if state.contains_command(envelope.command_id()) {
        return Err(KernelError::new(KernelErrorKind::DuplicateCommand));
    }
    if state.contains_event(envelope.event_id()) {
        return Err(KernelError::new(KernelErrorKind::DuplicateEvent));
    }
    match state.last_sequence.checked_next() {
        Ok(sequence) => Ok(sequence),
        Err(_) => Err(KernelError::new(KernelErrorKind::SequenceOverflow)),
    }
}
fn apply_command(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::PauseSession
        | KernelCommand::ResumeSession
        | KernelCommand::CloseSession => session::apply(state, command),
        KernelCommand::StartRun { .. }
        | KernelCommand::PauseRun { .. }
        | KernelCommand::ResumeRun { .. }
        | KernelCommand::CancelRun { .. }
        | KernelCommand::FailRun { .. }
        | KernelCommand::ExhaustRun { .. }
        | KernelCommand::RejectRun { .. } => run::apply(state, command, inputs),
        KernelCommand::StartAttempt { .. }
        | KernelCommand::ResumeAttempt { .. }
        | KernelCommand::SubmitAttempt { .. }
        | KernelCommand::FailAttempt { .. }
        | KernelCommand::ExhaustAttempt { .. } => attempt::apply(state, command, inputs),
        KernelCommand::StartTurn { .. }
        | KernelCommand::CompleteTurn { .. }
        | KernelCommand::FailTurn { .. }
        | KernelCommand::CancelTurn { .. } => turn::apply(state, command),
        KernelCommand::ProposeAction { .. }
        | KernelCommand::AuthorizeAction { .. }
        | KernelCommand::DispatchAction { .. }
        | KernelCommand::CompleteAction { .. }
        | KernelCommand::FailAction { .. }
        | KernelCommand::CancelAction { .. } => action::apply(state, command, inputs),
        KernelCommand::RequestReview { .. }
        | KernelCommand::BeginReview { .. }
        | KernelCommand::SubmitReview { .. }
        | KernelCommand::InvalidateReview { .. } => review::apply(state, command, inputs),
        KernelCommand::RequestWaiver { .. }
        | KernelCommand::GrantWaiver { .. }
        | KernelCommand::DenyWaiver { .. }
        | KernelCommand::InvalidateWaiver { .. } => waiver::apply(state, command, inputs),
        KernelCommand::BeginAcceptance { .. } | KernelCommand::EvaluateAcceptance { .. } => {
            acceptance::apply(state, command, inputs)
        }
    }
}
