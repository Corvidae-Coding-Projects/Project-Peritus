//! Exact completion rejection around a successful cancellation acknowledgement.

use super::{acknowledge_cancellation_outcome_matches, complete_command_outcome_matches};
use crate::state::mutation::{AcknowledgeCancellationOutcome, CompleteCommandOutcome};
use crate::{DispatchId, SchedulerCommandKind, SchedulerState, WorkId, WorkPhase, WorkTerminal};
use vstd::prelude::*;

mod classification;

use classification::{
    acknowledged_state_is_cancelled, acknowledgement_for, cancelling_dispatch, completion_for,
    completion_while_cancelling_is_rejected, ready_identities_are_unique,
};
verus! {
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
        exists |reservation_index: int, work_index: int| {
            &&& 0 <= reservation_index < cancelled.spec_reservations().len()
            &&& cancelled.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
            &&& cancelled.spec_reservations()[reservation_index].spec_work_id() == work_id
            &&& 0 <= work_index < cancelled.spec_work().len()
            &&& cancelled.spec_work()[work_index].spec_definition().spec_id() == work_id
            &&& cancelled.spec_work()[work_index].spec_phase() == WorkPhase::Cancelling
        },
        match late_completion {
            SchedulerCommandKind::CompleteWork { dispatch_id: requested, .. } => {
                *requested == dispatch_id
            },
            _ => false,
        },
        complete_command_outcome_matches(
            cancelled, after_late_completion, late_completion, late_outcome,
        ),
        match acknowledgement {
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: requested } => {
                *requested == dispatch_id
            },
            _ => false,
        },
        acknowledge_cancellation_outcome_matches(
            after_late_completion, acknowledged, acknowledgement, acknowledgement_outcome,
        ),
        match later_completion {
            SchedulerCommandKind::CompleteWork { dispatch_id: requested, .. } => {
                *requested == dispatch_id
            },
            _ => false,
        },
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
    assert(cancelling_dispatch(cancelled, dispatch_id, work_id));
    assert(completion_for(late_completion, dispatch_id));
    assert(acknowledgement_for(acknowledgement, dispatch_id));
    assert(completion_for(later_completion, dispatch_id));
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
