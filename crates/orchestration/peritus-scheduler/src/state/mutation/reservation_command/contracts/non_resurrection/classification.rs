//! Outcome classification helpers for cancellation non-resurrection.

use super::super::{
    acknowledge_cancellation_outcome_matches, complete_command_outcome_matches, reservation_at,
    work_at,
};
use crate::state::mutation::{AcknowledgeCancellationOutcome, CompleteCommandOutcome};
use crate::{DispatchId, SchedulerCommandKind, SchedulerState, WorkId, WorkPhase, WorkTerminal};
use vstd::prelude::*;

verus! {

pub open spec fn completion_for(command: &SchedulerCommandKind, dispatch_id: DispatchId) -> bool {
    match command {
        SchedulerCommandKind::CompleteWork { dispatch_id: requested, .. } => {
            *requested == dispatch_id
        },
        _ => false,
    }
}

pub open spec fn acknowledgement_for(
    command: &SchedulerCommandKind,
    dispatch_id: DispatchId,
) -> bool {
    match command {
        SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: requested } => {
            *requested == dispatch_id
        },
        _ => false,
    }
}

pub open spec fn cancelling_dispatch(
    state: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
) -> bool {
    exists |reservation_index: int, work_index: int| {
        &&& 0 <= reservation_index < state.spec_reservations().len()
        &&& state.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
        &&& state.spec_reservations()[reservation_index].spec_work_id() == work_id
        &&& 0 <= work_index < state.spec_work().len()
        &&& state.spec_work()[work_index].spec_definition().spec_id() == work_id
        &&& state.spec_work()[work_index].spec_phase() == WorkPhase::Cancelling
    }
}

pub(super) proof fn ready_identities_are_unique(state: &SchedulerState)
    requires state.spec_reservation_reducer_ready(),
    ensures
        crate::verified::reservation_identities_unique(state.spec_reservations()),
        crate::verified::work_identities_unique(state.spec_work()),
{
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservation_invariant_parts);
}

pub(super) proof fn completion_while_cancelling_is_rejected(
    before: &SchedulerState,
    after: &SchedulerState,
    command: &SchedulerCommandKind,
    outcome: CompleteCommandOutcome,
    dispatch_id: DispatchId,
    work_id: WorkId,
)
    requires
        before.spec_reservation_reducer_ready(),
        completion_for(command, dispatch_id),
        cancelling_dispatch(before, dispatch_id, work_id),
        complete_command_outcome_matches(before, after, command, outcome),
    ensures
        outcome == CompleteCommandOutcome::NotAcknowledgedRunning,
        *after == *before,
{
    ready_identities_are_unique(before);
    reveal(completion_for);
    reveal(cancelling_dispatch);
    let reservation_index = choose |reservation_index: int|
        #![trigger before.spec_reservations()[reservation_index]]
        exists |work_index: int| #![trigger before.spec_work()[work_index]] {
        &&& 0 <= reservation_index < before.spec_reservations().len()
        &&& before.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
        &&& before.spec_reservations()[reservation_index].spec_work_id() == work_id
        &&& 0 <= work_index < before.spec_work().len()
        &&& before.spec_work()[work_index].spec_definition().spec_id() == work_id
        &&& before.spec_work()[work_index].spec_phase() == WorkPhase::Cancelling
    };
    let work_index = choose |work_index: int| {
        &&& 0 <= work_index < before.spec_work().len()
        &&& before.spec_work()[work_index].spec_definition().spec_id() == work_id
        &&& before.spec_work()[work_index].spec_phase() == WorkPhase::Cancelling
    };
    reveal(complete_command_outcome_matches);
    reveal(crate::verified::reservation_identities_unique);
    reveal(crate::verified::work_identities_unique);
    match outcome {
        CompleteCommandOutcome::NotCompleteCommand => {},
        CompleteCommandOutcome::DispatchNotActive => {
            assert(before.spec_reservations()[reservation_index].spec_dispatch_id()
                != dispatch_id);
            assert(false);
        },
        CompleteCommandOutcome::WorkDisappeared => {
            let missing = choose |index: int|
                reservation_at(before, index, dispatch_id)
                    && forall |candidate: int| #![trigger before.spec_work()[candidate]]
                        0 <= candidate < before.spec_work().len() ==>
                            before.spec_work()[candidate].spec_definition().spec_id()
                                != before.spec_reservations()[index].spec_work_id();
            assert(missing == reservation_index);
            assert(false);
        },
        CompleteCommandOutcome::NotAcknowledgedRunning => {},
        CompleteCommandOutcome::Applied => {
            let applied_reservation = choose |reservation: int|
                #![trigger before.spec_reservations()[reservation]]
                exists |candidate: int|
                    #![trigger before.spec_work()[candidate]] {
                &&& reservation_at(before, reservation, dispatch_id)
                &&& work_at(
                    before, candidate,
                    before.spec_reservations()[reservation].spec_work_id(),
                )
                &&& before.spec_reservations()[reservation].spec_started()
                &&& before.spec_work()[candidate].spec_phase() == WorkPhase::Running
            };
            let applied_work = choose |candidate: int|
                #![trigger before.spec_work()[candidate]]
            {
                &&& reservation_at(before, applied_reservation, dispatch_id)
                &&& work_at(
                    before, candidate,
                    before.spec_reservations()[applied_reservation].spec_work_id(),
                )
                &&& before.spec_reservations()[applied_reservation].spec_started()
                &&& before.spec_work()[candidate].spec_phase() == WorkPhase::Running
            };
            reveal(reservation_at);
            reveal(work_at);
            assert(applied_reservation == reservation_index);
            assert(applied_work == work_index);
            assert(false);
        },
    }
}

pub(super) proof fn acknowledged_state_is_cancelled(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    removed: &crate::SchedulerReservation,
)
    requires
        before.spec_reservation_reducer_ready(),
        crate::state::mutation::terminal_release_matches(
            before, after, dispatch_id, work_id, WorkTerminal::Cancelled, removed,
        ),
    ensures
        forall |index: int| #![trigger after.spec_reservations()[index]]
            0 <= index < after.spec_reservations().len() ==>
                after.spec_reservations()[index].spec_dispatch_id() != dispatch_id,
        exists |index: int| #![trigger after.spec_work()[index]] {
            &&& 0 <= index < after.spec_work().len()
            &&& after.spec_work()[index].spec_definition().spec_id() == work_id
            &&& after.spec_work()[index].spec_phase() == WorkPhase::Terminal
            &&& after.spec_work()[index].spec_terminal() == Some(WorkTerminal::Cancelled)
        },
{
    ready_identities_are_unique(before);
    reveal(crate::state::mutation::terminal_release_matches);
    reveal(super::super::super::super::reservation_remove::exact_reservation_removal_matches);
    reveal(crate::verified::reservation_identities_unique);
    let removed_index = choose |index: int| #![trigger before.spec_reservations()[index]] {
        &&& 0 <= index < before.spec_reservations().len()
        &&& before.spec_reservations()[index].spec_dispatch_id() == dispatch_id
        &&& *removed == before.spec_reservations()[index]
        &&& after.spec_reservations() == before.spec_reservations().remove(index)
    };
    assert forall |index: int| #![trigger after.spec_reservations()[index]]
        0 <= index < after.spec_reservations().len()
        implies after.spec_reservations()[index].spec_dispatch_id() != dispatch_id by {
        if index < removed_index {
            assert(after.spec_reservations()[index] == before.spec_reservations()[index]);
            assert(index != removed_index);
        } else {
            assert(after.spec_reservations()[index] == before.spec_reservations()[index + 1]);
            assert(index + 1 != removed_index);
        }
    }
    reveal(crate::state::mutation::work_terminal_update_matches);
    let terminal_index = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& crate::state::mutation::work_record_update_matches(
            before.spec_work()[index], after.spec_work()[index], work_id, WorkPhase::Terminal,
        )
        &&& after.spec_work()[index].spec_terminal() == Some(WorkTerminal::Cancelled)
    };
    reveal(crate::state::mutation::work_record_update_matches);
    crate::WorkRecord::lifecycle_update_fields(
        &before.spec_work()[terminal_index],
        &after.spec_work()[terminal_index],
    );
    assert(exists |index: int| #![trigger after.spec_work()[index]] {
        &&& 0 <= index < after.spec_work().len()
        &&& after.spec_work()[index].spec_definition().spec_id() == work_id
        &&& after.spec_work()[index].spec_phase() == WorkPhase::Terminal
        &&& after.spec_work()[index].spec_terminal() == Some(WorkTerminal::Cancelled)
    }) by {
        assert(after.spec_work()[terminal_index].spec_definition().spec_id() == work_id);
    }
}

} // verus!
