//! Ordered worker-loss release batch and exact lost-worker transition.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

#[cfg(verus_only)]
use crate::LossOutcome;
use crate::{DispatchId, SchedulerEventKind, SchedulerState, WorkerId, WorkerPhase};

#[cfg(verus_only)]
use super::release_one::loss_release_matches;
#[cfg(verus_only)]
use super::selected_dispatches;

mod release_plan;
#[cfg(verus_only)]
mod selection;
#[cfg(verus_only)]
mod state_frame;
#[cfg(verus_only)]
mod trace;
mod worker_target;

#[cfg(verus_only)]
use release_plan::planned_dispatches_exist;
use release_plan::release_plan;
#[cfg(verus_only)]
use selection::{plan_dispatches_are_retained, selected_dispatches_are_unique};
#[cfg(verus_only)]
use worker_target::lost_worker_disappearance_is_impossible;
use worker_target::validate_worker_target;

verus! {

/// Existing worker-loss failure classes retained by the batch boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::reducer::apply) enum WorkerLossError {
    WorkerMissing,
    AlreadyLostOrRemoved,
    ReservationDisappeared,
    WorkDisappeared,
    LostWorkerDisappeared,
}

/// Ordered dispatch projection from the wrapper-supplied digest plan.
pub open spec fn plan_dispatches(
    plan: Seq<(DispatchId, Sha256Digest)>,
) -> Seq<DispatchId> {
    plan.map_values(|entry: (DispatchId, Sha256Digest)| entry.0)
}

/// A retained worker whose current phase permits worker-loss processing.
pub open spec fn worker_loss_target(state: &SchedulerState, worker_id: WorkerId) -> bool {
    exists |index: int| #![trigger state.spec_workers()[index]] {
        &&& 0 <= index < state.spec_workers().len()
        &&& state.spec_workers()[index].spec_descriptor().spec_id() == worker_id
        &&& state.spec_workers()[index].spec_phase() != WorkerPhase::Lost
        &&& state.spec_workers()[index].spec_phase() != WorkerPhase::Removed
    }
}

/// Exact sequence of release states and outcomes in plan order.
pub open spec fn release_trace_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    plan: Seq<(DispatchId, Sha256Digest)>,
    outcomes: Seq<LossOutcome>,
) -> bool {
    exists |states: Seq<SchedulerState>| {
        &&& outcomes.len() == plan.len()
        &&& states.len() == plan.len() + 1
        &&& states[0] == *before
        &&& states[plan.len() as int] == *after
        &&& forall |step: int| #![trigger states[step]] 0 <= step < plan.len() ==>
            loss_release_matches(
                &states[step],
                &states[step + 1],
                plan[step].0,
                plan[step].1,
                outcomes[step],
            )
    }
}

/// Exact public event payload emitted by successful worker-loss processing.
pub open spec fn worker_loss_event_matches(
    event: &SchedulerEventKind,
    worker_id: WorkerId,
    outcomes: Seq<LossOutcome>,
) -> bool {
    match event {
        SchedulerEventKind::WorkerLost { worker_id: observed, outcomes: emitted } => {
            *observed == worker_id && emitted@ == outcomes
        },
        _ => false,
    }
}

/// Complete state frame outside release effects and the worker lifecycle update.
pub open spec fn worker_loss_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
}

/// Complete frame for a release batch before the final worker phase update.
pub open spec fn release_batch_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    worker_loss_preserves_other_state(before, after)
        && after.spec_workers() == before.spec_workers()
}

/// Exact release trace, lost-worker lifecycle, event, and complete state frame.
pub open spec fn worker_loss_apply_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    worker_id: WorkerId,
    plan: Seq<(DispatchId, Sha256Digest)>,
    event: &SchedulerEventKind,
) -> bool {
    exists |released: SchedulerState, outcomes: Seq<LossOutcome>| {
        &&& release_trace_matches(before, &released, plan, outcomes)
        &&& crate::state::mutation::worker_phase_state_matches(
            &released, after, worker_id, WorkerPhase::Lost, true,
        )
        &&& worker_loss_event_matches(event, worker_id, outcomes)
        &&& worker_loss_preserves_other_state(before, after)
    }
}

/// Releases the exact precomputed dispatch/digest plan, then marks the worker lost.
///
/// Digest values are ordinary inputs. This boundary proves their exact propagation into terminal
/// payloads without making a claim about how the caller computed them.
pub(in crate::reducer::apply) fn apply_worker_loss(
    state: &mut SchedulerState,
    worker_id: WorkerId,
    plan: &[(DispatchId, Sha256Digest)],
) -> (result: Result<SchedulerEventKind, WorkerLossError>)
    ensures
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && worker_loss_target(old(state), worker_id)
                && plan_dispatches(plan@)
                    == selected_dispatches(old(state).spec_reservations(), worker_id)
            ==> match result {
                Ok(ref event) => {
                    &&& worker_loss_apply_matches(
                        old(state), final(state), worker_id, plan@, event,
                    )
                    &&& final(state).spec_reservation_reducer_ready()
                    &&& final(state).spec_collections_ordered()
                },
                Err(_) => false,
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && worker_loss_target(old(state), worker_id)
                && plan_dispatches(plan@)
                    == selected_dispatches(old(state).spec_reservations(), worker_id)
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    let ghost initial = *state;
    let ghost valid = state.spec_reservation_reducer_ready()
        && state.spec_collections_ordered()
        && worker_loss_target(state, worker_id)
        && plan_dispatches(plan@) == selected_dispatches(state.spec_reservations(), worker_id);
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    validate_worker_target(state, worker_id)?;
    proof {
        if valid {
            reveal(SchedulerState::spec_collections_ordered);
            selected_dispatches_are_unique(initial.spec_reservations(), worker_id);
            plan_dispatches_are_retained(&initial, worker_id, plan@);
            reveal(planned_dispatches_exist);
        }
    }
    let outcomes = match release_plan(state, plan) {
        Ok(outcomes) => outcomes,
        Err(error) => {
            proof { if valid { assert(false); } }
            return Err(error);
        },
    };
    let ghost released = *state;
    let ghost released_outcomes = outcomes@;
    let marked = crate::state::mutation::set_worker_phase(state, worker_id, WorkerPhase::Lost);
    if !marked {
        proof {
            if valid {
                lost_worker_disappearance_is_impossible(&initial, &released, state, worker_id);
            }
        }
        return Err(WorkerLossError::LostWorkerDisappeared);
    }
    let event = SchedulerEventKind::WorkerLost { worker_id, outcomes };
    proof {
        if valid {
            if had_queue_bound {
                reveal(crate::state::mutation::worker_phase_state_matches);
                reveal(crate::state::mutation::worker_update_preserves_other_state);
                reveal(crate::state::queue::queue_bound);
                reveal(crate::state::queue::admission_pressure);
            }
            reveal(worker_loss_event_matches);
            assert(worker_loss_event_matches(&event, worker_id, released_outcomes));
            trace::establish_worker_loss_apply(
                &initial, &released, state, worker_id, plan@, &event,
            );
        }
    }
    Ok(event)
}

} // verus!
