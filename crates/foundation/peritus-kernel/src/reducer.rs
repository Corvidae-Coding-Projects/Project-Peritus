//! Total authoritative lifecycle reducer.

mod acceptance;
mod action;
mod attempt;
mod review;
mod refinement;
mod run;
mod session;
mod turn;
mod waiver;
mod waiver_evidence;
#[cfg(verus_only)]
mod waiver_correspondence;

use crate::{
    AcceptanceOutcome, CommandEnvelope, KernelAggregate, KernelCommand, KernelError,
    KernelErrorKind, KernelEvent, KernelEventKind, KernelOutcome, KernelSubject, KernelTransition,
    ReducerInputs,
};
use peritus_types::EventSequence;
use vstd::prelude::*;

verus! {

struct AppliedCommand {
    event_kind: KernelEventKind,
    subject: KernelSubject,
    acceptance_outcome: Option<AcceptanceOutcome>,
}

impl AppliedCommand {
    const fn new(event_kind: KernelEventKind, subject: KernelSubject) -> (result: Self)
        ensures result.event_kind == event_kind, result.subject == subject,
            result.acceptance_outcome.is_none(),
    {
        Self { event_kind, subject, acceptance_outcome: None }
    }
    const fn acceptance(
        event_kind: KernelEventKind,
        subject: KernelSubject,
        outcome: AcceptanceOutcome,
    ) -> (result: Self)
        ensures result.event_kind == event_kind, result.subject == subject,
            result.acceptance_outcome == Some(outcome),
    {
        Self { event_kind, subject, acceptance_outcome: Some(outcome) }
    }
}

/// An applied evaluation acceptance is justified by the exact supplied policy inputs.
/// Other command families retain their separate lifecycle refinement obligations.
pub closed spec fn acceptance_result_authorized(
    before: &KernelAggregate,
    command: KernelCommand,
    inputs: &ReducerInputs<'_>,
    result: KernelOutcome,
) -> bool {
    match result {
        KernelOutcome::Applied(transition) =>
            matches!(command, KernelCommand::EvaluateAcceptance { .. })
            && transition.event.kind == KernelEventKind::AcceptanceAccepted
            ==> inputs.spec_acceptance_authorized(before.revision),
        KernelOutcome::Rejected { .. } => true,
    }
}

#[cfg(verus_only)]
pub use waiver_correspondence::{
    waiver_grant_authorized, waiver_grant_recorded, waiver_grant_result_authorized,
};

/// Direct refinement contract of the executable reducer.
pub closed spec fn reducer_result_refines(
    before: &KernelAggregate,
    envelope: CommandEnvelope,
    result: KernelOutcome,
) -> bool {
    match result {
        KernelOutcome::Rejected { aggregate, .. } => aggregate == *before,
        KernelOutcome::Applied(transition) => {
            crate::model::causal_transition_refines(before, envelope, &transition)
                && crate::model::legal_concrete_step(
                    before,
                    &transition.aggregate,
                    transition.event.kind,
                    transition.event.subject,
                )
                && (transition.event.kind == KernelEventKind::AcceptanceAccepted
                    || crate::model::no_new_accepted_run(
                        before,
                        &transition.aggregate,
                    ))
        }
    }
}

impl KernelAggregate {
    /// Applies one command against the exact current causal head.
    ///
    /// Every rejection returns this owned aggregate unchanged. Every accepted command returns one
    /// next aggregate and exactly one event plan.
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
    ) -> (result: KernelOutcome)
        ensures
            reducer_result_refines(&self, envelope, result),
            acceptance_result_authorized(&self, command, &inputs, result),
            waiver_grant_result_authorized(&self, command, &inputs, result),
    {
        let next_sequence = match preflight(&self, envelope, &inputs) {
            Ok(sequence) => sequence,
            Err(error) => return KernelOutcome::Rejected { aggregate: self, error },
        };
        let original = self;
        let mut next = original.clone();
        let previous_event_id = original.head_event_id;
        let ghost apply_input_checkpoint = next;
        let applied = match apply_command(&mut next, &command, &inputs) {
            Ok(applied) => applied,
            Err(error) => return KernelOutcome::Rejected { aggregate: original, error },
        };
        proof {
            if matches!(command, KernelCommand::EvaluateAcceptance { .. })
                && applied.event_kind == KernelEventKind::AcceptanceAccepted
            {
                acceptance::authorization_follows_clone(
                    &apply_input_checkpoint, &original, &inputs);
            }
            if let KernelCommand::GrantWaiver { finding_id } = &command {
                if applied.event_kind == KernelEventKind::WaiverGranted {
                    waiver_correspondence::grant_authorized_is_extensional(
                        &apply_input_checkpoint, &original, *finding_id, &inputs);
                    assert(waiver_grant_recorded(&next, *finding_id, &inputs));
                }
            }
        }
        let ghost grant_checkpoint = next;
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
        proof {
            if let KernelCommand::GrantWaiver { finding_id } = &command {
                if applied.event_kind == KernelEventKind::WaiverGranted {
                    grant_checkpoint.expose_internal_views();
                    next.expose_internal_views();
                    original.expose_internal_views();
                    waiver_correspondence::grant_revision_follows_clone(
                        &apply_input_checkpoint, &grant_checkpoint, &original);
                    assert(grant_checkpoint.spec_revision() == original.spec_revision());
                    waiver_correspondence::grant_recorded_is_extensional(
                        &grant_checkpoint, &next, *finding_id, &inputs);
                }
            }
        }
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
        proof {
            if let KernelCommand::GrantWaiver { finding_id } = &command {
                if applied.event_kind == KernelEventKind::WaiverGranted {
                    grant_checkpoint.expose_internal_views();
                    next.expose_internal_views();
                    waiver_correspondence::grant_recorded_is_extensional(
                        &grant_checkpoint, &next, *finding_id, &inputs);
                }
            }
        }
        let transition = KernelTransition::new(
            next,
            event,
            applied.acceptance_outcome,
        );
        proof {
            assert(crate::identity::revisions_equal(envelope.revision, original.revision));
            assert(crate::identity::optional_event_ids_equal(
                envelope.expected_previous_event_id,
                Some(original.head_event_id),
            ));
            assert(crate::identity::event_ids_equal(transition.event.id, envelope.event_id));
            assert(transition.event.command_id == envelope.command_id);
            assert(crate::identity::optional_event_ids_equal(
                transition.event.previous_event_id,
                Some(original.head_event_id),
            ));
            assert(crate::identity::revisions_equal(
                transition.event.revision,
                original.revision,
            ));
            assert(
                transition.event.sequence.spec_value()
                    == original.last_sequence.spec_value() + 1
            );
            assert(crate::identity::event_ids_equal(
                transition.aggregate.head_event_id,
                transition.event.id,
            ));
            assert(transition.aggregate.last_sequence == transition.event.sequence);
            assert(crate::identity::revisions_equal(
                transition.aggregate.revision,
                original.revision,
            ));
            assert(crate::model::causal_transition_refines(
                &original,
                envelope,
                &transition,
            ));
            assert(crate::model::legal_concrete_step(
                &original,
                &transition.aggregate,
                transition.event.kind,
                transition.event.subject,
            ));
            assert(
                transition.event.kind == KernelEventKind::AcceptanceAccepted
                    || crate::model::no_new_accepted_run(&original, &transition.aggregate)
            );
            if let KernelCommand::GrantWaiver { finding_id } = &command {
                if transition.event.kind == KernelEventKind::WaiverGranted {
                    assert(applied.event_kind == KernelEventKind::WaiverGranted);
                    assert(waiver_grant_recorded(
                        &transition.aggregate,
                        *finding_id,
                        &inputs,
                    ));
                }
            }
            assert(waiver_grant_result_authorized(
                &original,
                command,
                &inputs,
                KernelOutcome::Applied(transition),
            ));
        }
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
) -> (result: Result<EventSequence, KernelError>)
    ensures match result {
        Ok(sequence) => {
            sequence.spec_value() == state.last_sequence.spec_value() + 1
                && crate::identity::revisions_equal(envelope.revision, state.revision)
                && crate::identity::optional_event_ids_equal(
                    envelope.expected_previous_event_id,
                    Some(state.head_event_id),
                )
        }
        Err(_) => true,
    },
{
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
        Ok(sequence) => {
            assert(sequence.spec_value() == state.last_sequence.spec_value() + 1);
            Ok(sequence)
        }
        Err(_) => Err(KernelError::new(KernelErrorKind::SequenceOverflow)),
    }
}

fn apply_command(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> (result: Result<AppliedCommand, KernelError>)
    ensures match result {
        Ok(applied) => {
            &&& matches!(command, KernelCommand::EvaluateAcceptance { .. })
                && applied.event_kind == KernelEventKind::AcceptanceAccepted
                ==> inputs.spec_acceptance_authorized(old(state).revision)
            &&& match command {
                KernelCommand::GrantWaiver { finding_id } =>
                    applied.event_kind == KernelEventKind::WaiverGranted ==>
                        applied.subject == KernelSubject::Waiver(*finding_id)
                        && final(state).revision == old(state).revision
                        && waiver_grant_authorized(old(state), *finding_id, inputs)
                        && waiver_grant_recorded(final(state), *finding_id, inputs),
                _ => true,
            }
        }
        Err(_) => true,
    },
{
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
        KernelCommand::BeginAcceptance { .. }
        | KernelCommand::EvaluateAcceptance { .. } => acceptance::apply(state, command, inputs),
    }
}

} // verus!
