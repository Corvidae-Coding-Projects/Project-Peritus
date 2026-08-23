//! Closed mathematical lifecycle and concrete reducer-refinement models.

use crate::{
    AcceptancePhase, CommandEnvelope, KernelAggregate, KernelEventKind, KernelSubject,
    KernelTransition, RunPhase, SessionPhase,
};
use peritus_types::RunId;
use vstd::prelude::*;

verus! {

/// Minimal proof projection of the executable aggregate.
#[derive(Clone, Copy)]
pub(crate) struct KernelModelState {
    pub(crate) session: SessionPhase,
    pub(crate) run: Option<RunPhase>,
    pub(crate) acceptance: Option<AcceptancePhase>,
    pub(crate) sequence: int,
}

/// Global accepted-state invariant.
pub(crate) open spec fn accepted_state_is_complete(state: KernelModelState) -> bool {
    match (state.run, state.acceptance) {
        (Some(RunPhase::Accepted), Some(AcceptancePhase::Accepted)) => true,
        (Some(RunPhase::Accepted), _) | (_, Some(AcceptancePhase::Accepted)) => false,
        _ => true,
    }
}

/// Exact one-event sequence advancement.
pub(crate) open spec fn sequence_advances_once(
    before: KernelModelState,
    after: KernelModelState,
) -> bool {
    after.sequence == before.sequence + 1
}

/// Terminal/failure event kinds that must never produce acceptance.
pub(crate) open spec fn is_non_success_event(event: KernelEventKind) -> bool {
    matches!(
        event,
        KernelEventKind::SessionPaused
            | KernelEventKind::SessionClosed
            | KernelEventKind::RunPaused
            | KernelEventKind::RunCancelled
            | KernelEventKind::RunFailed
            | KernelEventKind::RunExhausted
            | KernelEventKind::RunRejected
            | KernelEventKind::AttemptFailed
            | KernelEventKind::AttemptExhausted
            | KernelEventKind::TurnFailed
            | KernelEventKind::TurnCancelled
            | KernelEventKind::ActionFailed
            | KernelEventKind::ActionCancelled
            | KernelEventKind::AcceptanceNeedsChanges
    )
}

/// Closed legal transition relation for proof-visible session/run/acceptance state.
pub(crate) open spec fn legal_model_step(
    before: KernelModelState,
    event: KernelEventKind,
    after: KernelModelState,
) -> bool {
    sequence_advances_once(before, after)
        && accepted_state_is_complete(after)
        && (after.run == Some(RunPhase::Accepted)
            ==> event == KernelEventKind::AcceptanceAccepted)
        && (!is_non_success_event(event)
            || after.run != Some(RunPhase::Accepted))
        && match event {
            KernelEventKind::SessionPaused => {
                before.session == SessionPhase::Open && after.session == SessionPhase::Paused
            }
            KernelEventKind::SessionResumed => {
                before.session == SessionPhase::Paused && after.session == SessionPhase::Open
            }
            KernelEventKind::SessionClosed => after.session == SessionPhase::Closed,
            KernelEventKind::AcceptanceAccepted => {
                before.run == Some(RunPhase::Reviewing)
                    && before.acceptance == Some(AcceptancePhase::Evaluating)
                    && after.run == Some(RunPhase::Accepted)
                    && after.acceptance == Some(AcceptancePhase::Accepted)
            }
            KernelEventKind::AcceptanceNeedsChanges => {
                before.run == Some(RunPhase::Reviewing)
                    && before.acceptance == Some(AcceptancePhase::Evaluating)
                    && after.run == Some(RunPhase::Fixing)
                    && after.acceptance == Some(AcceptancePhase::NeedsChanges)
            }
            _ => true,
        }
}

/// A concrete run with `id` moves through the exact acceptance-success edge.
pub(crate) open spec fn concrete_acceptance_succeeds(
    before: &KernelAggregate,
    after: &KernelAggregate,
    id: RunId,
) -> bool {
    exists |before_index: int, after_index: int| {
        &&& 0 <= before_index < before.runs@.len()
        &&& 0 <= after_index < after.runs@.len()
        &&& crate::identity::run_ids_equal(before.runs@[before_index].id, id)
        &&& crate::identity::run_ids_equal(after.runs@[after_index].id, id)
        &&& before.runs@[before_index].phase == RunPhase::Reviewing
        &&& before.runs@[before_index].acceptance == AcceptancePhase::Evaluating
        &&& after.runs@[after_index].phase == RunPhase::Accepted
        &&& after.runs@[after_index].acceptance == AcceptancePhase::Accepted
    }
}

/// A concrete run with `id` moves through the exact needs-changes edge.
pub(crate) open spec fn concrete_acceptance_needs_changes(
    before: &KernelAggregate,
    after: &KernelAggregate,
    id: RunId,
) -> bool {
    exists |before_index: int, after_index: int| {
        &&& 0 <= before_index < before.runs@.len()
        &&& 0 <= after_index < after.runs@.len()
        &&& crate::identity::run_ids_equal(before.runs@[before_index].id, id)
        &&& crate::identity::run_ids_equal(after.runs@[after_index].id, id)
        &&& before.runs@[before_index].phase == RunPhase::Reviewing
        &&& before.runs@[before_index].acceptance == AcceptancePhase::Evaluating
        &&& after.runs@[after_index].phase == RunPhase::Fixing
        &&& after.runs@[after_index].acceptance == AcceptancePhase::NeedsChanges
    }
}

/// Concrete safety-critical phase relation audited around every applied reducer command.
pub(crate) open spec fn legal_concrete_step(
    before: &KernelAggregate,
    after: &KernelAggregate,
    event: KernelEventKind,
    subject: KernelSubject,
) -> bool {
    match (event, subject) {
        (KernelEventKind::SessionPaused, KernelSubject::Session(_)) => {
            before.session.phase == SessionPhase::Open
                && after.session.phase == SessionPhase::Paused
        }
        (KernelEventKind::SessionResumed, KernelSubject::Session(_)) => {
            before.session.phase == SessionPhase::Paused
                && after.session.phase == SessionPhase::Open
        }
        (KernelEventKind::SessionClosed, KernelSubject::Session(_)) => {
            before.session.phase != SessionPhase::Closed
                && after.session.phase == SessionPhase::Closed
        }
        (KernelEventKind::AcceptanceAccepted, KernelSubject::Acceptance(id)) => {
            concrete_acceptance_succeeds(before, after, id)
        }
        (KernelEventKind::AcceptanceNeedsChanges, KernelSubject::Acceptance(id)) => {
            concrete_acceptance_needs_changes(before, after, id)
        }
        (KernelEventKind::SessionPaused, _)
        | (KernelEventKind::SessionResumed, _)
        | (KernelEventKind::SessionClosed, _)
        | (KernelEventKind::AcceptanceAccepted, _)
        | (KernelEventKind::AcceptanceNeedsChanges, _) => false,
        _ => true,
    }
}

/// Every accepted run in `after` was already accepted in `before`.
pub(crate) open spec fn no_new_accepted_run(
    before: &KernelAggregate,
    after: &KernelAggregate,
) -> bool {
    forall |after_index: int| #![auto]
        0 <= after_index < after.runs@.len()
            && after.runs@[after_index].phase == RunPhase::Accepted
        ==> exists |before_index: int| {
            &&& 0 <= before_index < before.runs@.len()
            &&& crate::identity::run_ids_equal(
                before.runs@[before_index].id,
                after.runs@[after_index].id,
            )
            &&& before.runs@[before_index].phase == RunPhase::Accepted
        }
}

/// Exact causal and revision metadata for one applied transition.
pub(crate) open spec fn causal_transition_refines(
    before: &KernelAggregate,
    envelope: CommandEnvelope,
    transition: &KernelTransition,
) -> bool {
    let event = transition.event;
    let after = &transition.aggregate;
    &&& crate::identity::revisions_equal(envelope.revision, before.revision)
    &&& crate::identity::optional_event_ids_equal(
        envelope.expected_previous_event_id,
        Some(before.head_event_id),
    )
    &&& crate::identity::event_ids_equal(event.id, envelope.event_id)
    &&& event.command_id == envelope.command_id
    &&& crate::identity::optional_event_ids_equal(
        event.previous_event_id,
        Some(before.head_event_id),
    )
    &&& crate::identity::revisions_equal(event.revision, before.revision)
    &&& event.sequence.spec_value() == before.last_sequence.spec_value() + 1
    &&& crate::identity::event_ids_equal(after.head_event_id, event.id)
    &&& after.last_sequence == event.sequence
    &&& crate::identity::revisions_equal(after.revision, before.revision)
}

} // verus!
