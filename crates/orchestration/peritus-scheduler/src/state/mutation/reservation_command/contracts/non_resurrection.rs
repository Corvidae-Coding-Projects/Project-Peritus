//! Exact completion rejection around a successful cancellation acknowledgement.

use super::{acknowledge_cancellation_outcome_matches, complete_command_outcome_matches};
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

proof fn ready_identities_are_unique(state: &SchedulerState)
    requires state.spec_reservation_reducer_ready(),
    ensures
        crate::verified::reservation_identities_unique(state.spec_reservations()),
        crate::verified::work_identities_unique(state.spec_work()),
{
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservation_invariant_parts);
}

proof fn completion_while_cancelling_is_rejected(
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
                super::reservation_at(before, index, dispatch_id)
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
                &&& super::reservation_at(before, reservation, dispatch_id)
                &&& super::work_at(
                    before, candidate,
                    before.spec_reservations()[reservation].spec_work_id(),
                )
                &&& before.spec_reservations()[reservation].spec_started()
                &&& before.spec_work()[candidate].spec_phase() == WorkPhase::Running
            };
            let applied_work = choose |candidate: int|
                #![trigger before.spec_work()[candidate]]
            {
                &&& super::reservation_at(before, applied_reservation, dispatch_id)
                &&& super::work_at(
                    before, candidate,
                    before.spec_reservations()[applied_reservation].spec_work_id(),
                )
                &&& before.spec_reservations()[applied_reservation].spec_started()
                &&& before.spec_work()[candidate].spec_phase() == WorkPhase::Running
            };
            reveal(super::reservation_at);
            reveal(super::work_at);
            assert(applied_reservation == reservation_index);
            assert(applied_work == work_index);
            assert(false);
        },
    }
}

proof fn acknowledged_state_is_cancelled(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    removed: &crate::SchedulerReservation,
)
    requires
        before.spec_reservation_reducer_ready(),
        super::super::super::terminal_release_matches(
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
    reveal(super::super::super::terminal_release_matches);
    reveal(super::super::super::reservation_remove::exact_reservation_removal_matches);
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
    reveal(super::super::super::work_terminal_update_matches);
    let terminal_index = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& super::super::super::work_record_update_matches(
            before.spec_work()[index], after.spec_work()[index], work_id, WorkPhase::Terminal,
        )
        &&& after.spec_work()[index].spec_terminal() == Some(WorkTerminal::Cancelled)
    };
    reveal(super::super::super::work_record_update_matches);
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

/// The actual command relations exclude both races around cancellation acknowledgement.
pub(crate) proof fn cancelling_dispatch_cannot_resurrect(
    cancelled: &SchedulerState,
    after_late_completion: &SchedulerState,
    acknowledged: &SchedulerState,
    final_state: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    late_completion: &SchedulerCommandKind,
    late_outcome: CompleteCommandOutcome,
    acknowledgement: &SchedulerCommandKind,
    acknowledgement_outcome: AcknowledgeCancellationOutcome,
    later_completion: &SchedulerCommandKind,
    later_outcome: CompleteCommandOutcome,
)
    requires
        cancelled.spec_reservation_reducer_ready(),
        cancelled.spec_collections_ordered(),
        cancelling_dispatch(cancelled, dispatch_id, work_id),
        completion_for(late_completion, dispatch_id),
        complete_command_outcome_matches(
            cancelled, after_late_completion, late_completion, late_outcome,
        ),
        acknowledgement_for(acknowledgement, dispatch_id),
        acknowledge_cancellation_outcome_matches(
            after_late_completion, acknowledged, acknowledgement, acknowledgement_outcome,
        ),
        completion_for(later_completion, dispatch_id),
        complete_command_outcome_matches(
            acknowledged, final_state, later_completion, later_outcome,
        ),
    ensures
        late_outcome == CompleteCommandOutcome::NotAcknowledgedRunning,
        *after_late_completion == *cancelled,
        acknowledgement_outcome == AcknowledgeCancellationOutcome::Applied,
        later_outcome == CompleteCommandOutcome::DispatchNotActive,
        *final_state == *acknowledged,
        forall |index: int| #![trigger acknowledged.spec_reservations()[index]]
            0 <= index < acknowledged.spec_reservations().len() ==>
                acknowledged.spec_reservations()[index].spec_dispatch_id() != dispatch_id,
        exists |index: int| #![trigger acknowledged.spec_work()[index]] {
            &&& 0 <= index < acknowledged.spec_work().len()
            &&& acknowledged.spec_work()[index].spec_definition().spec_id() == work_id
            &&& acknowledged.spec_work()[index].spec_phase() == WorkPhase::Terminal
            &&& acknowledged.spec_work()[index].spec_terminal() == Some(WorkTerminal::Cancelled)
        },
{
    completion_while_cancelling_is_rejected(
        cancelled, after_late_completion, late_completion, late_outcome, dispatch_id, work_id,
    );
    assert(cancelling_dispatch(after_late_completion, dispatch_id, work_id));
    ready_identities_are_unique(after_late_completion);
    reveal(acknowledgement_for);
    reveal(acknowledge_cancellation_outcome_matches);
    reveal(cancelling_dispatch);
    let reservation_index = choose |reservation_index: int|
        #![trigger after_late_completion.spec_reservations()[reservation_index]]
        exists |work_index: int|
            #![trigger after_late_completion.spec_work()[work_index]] {
        &&& 0 <= reservation_index < after_late_completion.spec_reservations().len()
        &&& after_late_completion.spec_reservations()[reservation_index].spec_dispatch_id()
            == dispatch_id
        &&& after_late_completion.spec_reservations()[reservation_index].spec_work_id() == work_id
        &&& 0 <= work_index < after_late_completion.spec_work().len()
        &&& after_late_completion.spec_work()[work_index].spec_definition().spec_id() == work_id
        &&& after_late_completion.spec_work()[work_index].spec_phase() == WorkPhase::Cancelling
    };
    let work_index = choose |work_index: int| {
        &&& 0 <= work_index < after_late_completion.spec_work().len()
        &&& after_late_completion.spec_work()[work_index].spec_definition().spec_id() == work_id
        &&& after_late_completion.spec_work()[work_index].spec_phase() == WorkPhase::Cancelling
    };
    reveal(crate::verified::reservation_identities_unique);
    reveal(crate::verified::work_identities_unique);
    match acknowledgement_outcome {
        AcknowledgeCancellationOutcome::NotAcknowledgeCancellationCommand => {},
        AcknowledgeCancellationOutcome::DispatchNotActive => { assert(false); },
        AcknowledgeCancellationOutcome::ReservationWorkDisappeared => {
            let missing = choose |index: int|
                super::reservation_at(after_late_completion, index, dispatch_id)
                    && forall |candidate: int|
                        #![trigger after_late_completion.spec_work()[candidate]]
                        0 <= candidate < after_late_completion.spec_work().len() ==>
                            after_late_completion.spec_work()[candidate]
                                .spec_definition().spec_id()
                                != after_late_completion.spec_reservations()[index].spec_work_id();
            reveal(super::reservation_at);
            assert(missing == reservation_index);
            assert(false);
        },
        AcknowledgeCancellationOutcome::WorkNotCancelling => {
            let rejected_reservation = choose |reservation: int|
                #![trigger after_late_completion.spec_reservations()[reservation]]
                exists |candidate: int|
                    #![trigger after_late_completion.spec_work()[candidate]] {
                &&& super::reservation_at(after_late_completion, reservation, dispatch_id)
                &&& super::work_at(
                    after_late_completion, candidate,
                    after_late_completion.spec_reservations()[reservation].spec_work_id(),
                )
                &&& after_late_completion.spec_work()[candidate].spec_phase()
                    != WorkPhase::Cancelling
            };
            let rejected_work = choose |candidate: int| {
                &&& super::reservation_at(
                    after_late_completion, rejected_reservation, dispatch_id,
                )
                &&& super::work_at(
                    after_late_completion, candidate,
                    after_late_completion.spec_reservations()[rejected_reservation].spec_work_id(),
                )
                &&& after_late_completion.spec_work()[candidate].spec_phase()
                    != WorkPhase::Cancelling
            };
            reveal(super::reservation_at);
            reveal(super::work_at);
            assert(rejected_reservation == reservation_index);
            assert(rejected_work == work_index);
            assert(false);
        },
        AcknowledgeCancellationOutcome::CancellingWorkDisappeared => {},
        AcknowledgeCancellationOutcome::Applied => {},
    }
    let applied_reservation = choose |reservation: int|
        #![trigger after_late_completion.spec_reservations()[reservation]]
        exists |work: int| #![trigger after_late_completion.spec_work()[work]] {
        &&& super::reservation_at(after_late_completion, reservation, dispatch_id)
        &&& super::work_at(
            after_late_completion, work,
            after_late_completion.spec_reservations()[reservation].spec_work_id(),
        )
        &&& after_late_completion.spec_work()[work].spec_phase() == WorkPhase::Cancelling
    };
    let applied_work = choose |work: int|
        #![trigger after_late_completion.spec_work()[work]] {
        &&& super::reservation_at(after_late_completion, applied_reservation, dispatch_id)
        &&& super::work_at(
            after_late_completion, work,
            after_late_completion.spec_reservations()[applied_reservation].spec_work_id(),
        )
        &&& after_late_completion.spec_work()[work].spec_phase() == WorkPhase::Cancelling
    };
    reveal(super::reservation_at);
    reveal(super::work_at);
    assert(applied_reservation == reservation_index);
    assert(applied_work == work_index);
    assert(exists |removed: crate::SchedulerReservation|
        #![trigger super::super::super::terminal_release_matches(
            after_late_completion, acknowledged, dispatch_id, work_id,
            WorkTerminal::Cancelled, &removed,
        )]
        super::super::super::terminal_release_matches(
            after_late_completion, acknowledged, dispatch_id, work_id,
            WorkTerminal::Cancelled, &removed,
        ));
    let removed = choose |removed: crate::SchedulerReservation|
        #![trigger super::super::super::terminal_release_matches(
            after_late_completion, acknowledged, dispatch_id, work_id,
            WorkTerminal::Cancelled, &removed,
        )]
        super::super::super::terminal_release_matches(
            after_late_completion, acknowledged, dispatch_id, work_id,
            WorkTerminal::Cancelled, &removed,
        );
    acknowledged_state_is_cancelled(
        after_late_completion, acknowledged, dispatch_id, work_id, &removed,
    );
    reveal(completion_for);
    reveal(complete_command_outcome_matches);
    match later_outcome {
        CompleteCommandOutcome::NotCompleteCommand => {},
        CompleteCommandOutcome::DispatchNotActive => {},
        CompleteCommandOutcome::WorkDisappeared
        | CompleteCommandOutcome::NotAcknowledgedRunning
        | CompleteCommandOutcome::Applied => { assert(false); },
    }
}

} // verus!
