//! One exact reservation release during worker-loss application.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

#[cfg(verus_only)]
use crate::SchedulerReservation;
use crate::state::mutation;
use crate::{DispatchId, LossOutcome, SchedulerState, WorkPhase, WorkTerminal};

use super::classify;
#[cfg(verus_only)]
use super::outcome_matches;

#[cfg(verus_only)]
mod lookup_proofs;
#[cfg(verus_only)]
mod queue;
#[cfg(verus_only)]
use lookup_proofs::{
    establish_loss_release, missing_dispatch_is_impossible, missing_work_is_impossible,
};
#[cfg(verus_only)]
use queue::loss_release_preserves_queue_bound;

verus! {

/// Existing worker-loss lookup failure classes preserved by the extracted kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::reducer::apply) enum LossReleaseError {
    ReservationDisappeared,
    WorkDisappeared,
}

/// A live reservation with the requested dispatch is present.
pub open spec fn dispatch_exists(state: &SchedulerState, dispatch_id: DispatchId) -> bool {
    exists |index: int| #![trigger state.spec_reservations()[index]]
        0 <= index < state.spec_reservations().len()
            && state.spec_reservations()[index].spec_dispatch_id() == dispatch_id
}

/// The exact lifecycle mutation selected by one classified loss outcome.
pub open spec fn release_effect_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
    removed: &SchedulerReservation,
) -> bool {
    match outcome {
        LossOutcome::Requeued { dispatch_id: observed, work_id } => {
            &&& observed == dispatch_id
            &&& mutation::phase_release_matches(
                before, after, dispatch_id, work_id, WorkPhase::Queued, removed,
            )
        },
        LossOutcome::Cancelled { dispatch_id: observed, work_id } => {
            &&& observed == dispatch_id
            &&& mutation::terminal_release_matches(
                before, after, dispatch_id, work_id, WorkTerminal::Cancelled, removed,
            )
        },
        LossOutcome::Exhausted { dispatch_id: observed, work_id } => {
            &&& observed == dispatch_id
            &&& mutation::terminal_release_matches(
                before,
                after,
                dispatch_id,
                work_id,
                WorkTerminal::Exhausted { cause_digest: failure_digest },
                removed,
            )
        },
        LossOutcome::Ambiguous { dispatch_id: observed, work_id } => {
            &&& observed == dispatch_id
            &&& mutation::terminal_release_matches(
                before,
                after,
                dispatch_id,
                work_id,
                WorkTerminal::Ambiguous { dispatch_id },
                removed,
            )
        },
        LossOutcome::Failed { dispatch_id: observed, work_id } => {
            &&& observed == dispatch_id
            &&& mutation::terminal_release_matches(
                before,
                after,
                dispatch_id,
                work_id,
                WorkTerminal::Failed { failure_digest },
                removed,
            )
        },
    }
}

/// Exact old work classification, ownership removal, and lifecycle result for one loss.
pub open spec fn loss_release_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
) -> bool {
    exists |work_index: int, removed: SchedulerReservation| {
        &&& 0 <= work_index < before.spec_work().len()
        &&& outcome_matches(before.spec_work()[work_index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )
    }
}

/// Classifies and releases one live reservation from the pre-mutation worker-loss snapshot.
pub(in crate::reducer::apply) fn release_one(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
) -> (result: Result<LossOutcome, LossReleaseError>)
    ensures
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && dispatch_exists(old(state), dispatch_id)
            ==> match result {
                Ok(outcome) => {
                    &&& loss_release_matches(
                        old(state), final(state), dispatch_id, failure_digest, outcome,
                    )
                    &&& final(state).spec_reservation_reducer_ready()
                    &&& final(state).spec_collections_ordered()
                },
                Err(_) => false,
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && dispatch_exists(old(state), dispatch_id)
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    let ghost before_work = state.spec_work();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost was_ordered = state.spec_collections_ordered();
    let ghost existed = dispatch_exists(state, dispatch_id);
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    let Some(reservation) = state.reservation(dispatch_id) else {
        proof {
            if was_ready && was_ordered && existed {
                reveal(SchedulerState::spec_collections_ordered);
                missing_dispatch_is_impossible(state, dispatch_id);
            }
        }
        return Err(LossReleaseError::ReservationDisappeared);
    };
    let work_id = reservation.work_id();
    proof {
        reveal(mutation::release_target_exists);
        assert(exists |index: int| #![trigger state.spec_reservations()[index]]
            0 <= index < state.spec_reservations().len()
                && state.spec_reservations()[index].spec_dispatch_id() == dispatch_id
                && state.spec_reservations()[index].spec_work_id() == work_id);
    }
    let Some(record) = state.work_item(work_id) else {
        proof {
            if was_ready && was_ordered && existed {
                reveal(SchedulerState::spec_collections_ordered);
                missing_work_is_impossible(state, dispatch_id, work_id);
            }
        }
        return Err(LossReleaseError::WorkDisappeared);
    };
    let ghost selected_work = *record;
    let outcome = classify(record, dispatch_id);
    let released = match &outcome {
        LossOutcome::Requeued { .. } => {
            mutation::release_to_phase(state, dispatch_id, work_id, WorkPhase::Queued)
        },
        LossOutcome::Cancelled { .. } => mutation::release_to_terminal(
            state, dispatch_id, work_id, WorkTerminal::Cancelled,
        ),
        LossOutcome::Exhausted { .. } => mutation::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Exhausted { cause_digest: failure_digest },
        ),
        LossOutcome::Ambiguous { .. } => mutation::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Ambiguous { dispatch_id },
        ),
        LossOutcome::Failed { .. } => mutation::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Failed { failure_digest },
        ),
    };
    let Some(_removed) = released else {
        proof {
            if was_ready && was_ordered && existed {
                reveal(crate::verified::work_phase_retains_reservation);
                assert(false);
            }
        }
        return Err(LossReleaseError::WorkDisappeared);
    };
    proof {
        if was_ready && was_ordered && existed {
            reveal(crate::verified::work_phase_retains_reservation);
            reveal(release_effect_matches);
            reveal(outcome_matches);
            let work_index = choose |index: int| #![trigger before_work[index]]
                0 <= index < before_work.len() && before_work[index] == selected_work;
            establish_loss_release(
                old(state),
                state,
                dispatch_id,
                failure_digest,
                outcome,
                &_removed,
                work_index,
                selected_work,
            );
            if had_queue_bound {
                loss_release_preserves_queue_bound(
                    old(state), state, dispatch_id, failure_digest, outcome,
                );
            }
        }
    }
    Ok(outcome)
}

} // verus!
