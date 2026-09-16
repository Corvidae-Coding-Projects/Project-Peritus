//! Cancellation acknowledgement through the exact reservation release kernel.

use vstd::prelude::*;

use super::AcknowledgeCancellationOutcome;
#[cfg(verus_only)]
use super::{acknowledge_cancellation_outcome_matches, reservation_at, work_at};
use crate::{SchedulerCommandKind, SchedulerState, WorkPhase, WorkTerminal};

verus! {

/// Applies the actual `AcknowledgeCancellation` payload through the verified release mutation.
pub fn apply_acknowledge_cancellation_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> (outcome: AcknowledgeCancellationOutcome)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
            ==> acknowledge_cancellation_outcome_matches(
                old(state), final(state), command, outcome,
            ),
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
            ==> final(state).spec_collections_ordered(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
{
    let SchedulerCommandKind::AcknowledgeCancellation {
        dispatch_id: requested_dispatch,
    } = command else {
        proof { reveal(acknowledge_cancellation_outcome_matches); }
        return AcknowledgeCancellationOutcome::NotAcknowledgeCancellationCommand;
    };
    let dispatch_id = *requested_dispatch;
    let ghost before_reservations = state.spec_reservations();
    let ghost before_work = state.spec_work();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost was_ordered = state.spec_collections_ordered();
    let Some(reservation) = state.reservation(dispatch_id) else {
        proof {
            reveal(SchedulerState::spec_collections_ordered);
            reveal(acknowledge_cancellation_outcome_matches);
        }
        return AcknowledgeCancellationOutcome::DispatchNotActive;
    };
    let ghost reservation_index = choose |index: int|
        #![trigger before_reservations[index]]
        0 <= index < before_reservations.len()
            && before_reservations[index].spec_dispatch_id() == dispatch_id
            && before_reservations[index] == *reservation;
    let work_id = reservation.work_id();
    let Some(work) = state.work_item(work_id) else {
        proof {
            reveal(SchedulerState::spec_collections_ordered);
            reveal(reservation_at);
            reveal(acknowledge_cancellation_outcome_matches);
        }
        return AcknowledgeCancellationOutcome::ReservationWorkDisappeared;
    };
    let phase = work.phase();
    let ghost work_index = choose |index: int|
        #![trigger before_work[index]]
        0 <= index < before_work.len()
            && before_work[index].spec_definition().spec_id() == work_id
            && before_work[index] == *work;
    if !phase.same(WorkPhase::Cancelling) {
        proof {
            reveal(reservation_at);
            reveal(work_at);
            assert(old(state).spec_reservations()[reservation_index].spec_work_id() == work_id);
            super::contracts::establish_work_not_cancelling(
                old(state), command, dispatch_id, reservation_index, work_index,
            );
        }
        return AcknowledgeCancellationOutcome::WorkNotCancelling;
    }
    proof {
        reveal(super::super::release_target_exists);
        assert(exists |reservation_index: int|
            #![trigger state.spec_reservations()[reservation_index]]
            0 <= reservation_index < state.spec_reservations().len()
                && state.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
                && state.spec_reservations()[reservation_index].spec_work_id() == work_id);
    }
    let Some(_removed) = super::super::release_to_terminal(
        state, dispatch_id, work_id, WorkTerminal::Cancelled,
    ) else {
        proof { if was_ready { assert(false); } }
        return AcknowledgeCancellationOutcome::CancellingWorkDisappeared;
    };
    proof {
        if was_ready && was_ordered {
            assert(before_reservations == old(state).spec_reservations());
            assert(before_work == old(state).spec_work());
            assert(super::super::terminal_release_matches(
                old(state), state, dispatch_id, work_id,
                WorkTerminal::Cancelled, &_removed,
            ));
            reveal(reservation_at);
            reveal(work_at);
            reveal(acknowledge_cancellation_outcome_matches);
        }
    }
    AcknowledgeCancellationOutcome::Applied
}

} // verus!
