//! Exact state and outcome relations for reservation command kernels.

use vstd::prelude::*;

use super::{AcknowledgeCancellationOutcome, CompleteCommandOutcome};
use crate::{
    DispatchId, SchedulerCommandKind, SchedulerReservation, SchedulerState, WorkId, WorkPhase,
    WorkTerminal,
};

#[cfg(verus_only)]
mod non_resurrection;
#[cfg(verus_only)]
pub(crate) use non_resurrection::cancelling_dispatch_cannot_resurrect;

verus! {

pub open spec fn reservation_at(
    state: &SchedulerState,
    index: int,
    dispatch_id: DispatchId,
) -> bool {
    0 <= index < state.spec_reservations().len()
        && state.spec_reservations()[index].spec_dispatch_id() == dispatch_id
}

pub open spec fn work_at(
    state: &SchedulerState,
    index: int,
    work_id: WorkId,
) -> bool {
    0 <= index < state.spec_work().len()
        && state.spec_work()[index].spec_definition().spec_id() == work_id
}

/// Relates the exact result and state effect of one production completion command.
pub open spec fn complete_command_outcome_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    command: &SchedulerCommandKind,
    outcome: CompleteCommandOutcome,
) -> bool {
    &&& before.spec_reservation_reducer_ready() ==> after.spec_reservation_reducer_ready()
    &&& before.spec_reservation_reducer_ready() && before.spec_collections_ordered()
        ==> after.spec_collections_ordered()
    &&& match (command, outcome) {
        (SchedulerCommandKind::CompleteWork { .. }, CompleteCommandOutcome::NotCompleteCommand) => {
            false
        },
        (_, CompleteCommandOutcome::NotCompleteCommand) => *after == *before,
        (
            SchedulerCommandKind::CompleteWork { dispatch_id, .. },
            CompleteCommandOutcome::DispatchNotActive,
        ) => {
            &&& forall |index: int| #![trigger before.spec_reservations()[index]]
                0 <= index < before.spec_reservations().len() ==>
                    before.spec_reservations()[index].spec_dispatch_id() != *dispatch_id
            &&& *after == *before
        },
        (
            SchedulerCommandKind::CompleteWork { dispatch_id, .. },
            CompleteCommandOutcome::WorkDisappeared,
        ) => {
            &&& exists |reservation_index: int|
                reservation_at(before, reservation_index, *dispatch_id)
                    && forall |work_index: int| #![trigger before.spec_work()[work_index]]
                        0 <= work_index < before.spec_work().len() ==>
                            before.spec_work()[work_index].spec_definition().spec_id()
                                != before.spec_reservations()[reservation_index].spec_work_id()
            &&& *after == *before
        },
        (
            SchedulerCommandKind::CompleteWork { dispatch_id, .. },
            CompleteCommandOutcome::NotAcknowledgedRunning,
        ) => {
            &&& exists |reservation_index: int, work_index: int|
                reservation_at(before, reservation_index, *dispatch_id)
                    && work_at(
                        before,
                        work_index,
                        before.spec_reservations()[reservation_index].spec_work_id(),
                    )
                    && (!before.spec_reservations()[reservation_index].spec_started()
                        || before.spec_work()[work_index].spec_phase() != WorkPhase::Running)
            &&& *after == *before
        },
        (
            SchedulerCommandKind::CompleteWork { dispatch_id, result_digest },
            CompleteCommandOutcome::Applied,
        ) => exists |reservation_index: int, work_index: int, removed: SchedulerReservation|
            reservation_at(before, reservation_index, *dispatch_id)
                && work_at(
                    before,
                    work_index,
                    before.spec_reservations()[reservation_index].spec_work_id(),
                )
                && before.spec_reservations()[reservation_index].spec_started()
                && before.spec_work()[work_index].spec_phase() == WorkPhase::Running
                && super::super::terminal_release_matches(
                    before,
                    after,
                    *dispatch_id,
                    before.spec_reservations()[reservation_index].spec_work_id(),
                    WorkTerminal::Succeeded { result_digest: *result_digest },
                    &removed,
                ),
        (_, _) => false,
    }
}

/// Relates the exact result and state effect of one cancellation acknowledgement.
pub open spec fn acknowledge_cancellation_outcome_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    command: &SchedulerCommandKind,
    outcome: AcknowledgeCancellationOutcome,
) -> bool {
    &&& before.spec_reservation_reducer_ready() ==> after.spec_reservation_reducer_ready()
    &&& before.spec_reservation_reducer_ready() && before.spec_collections_ordered()
        ==> after.spec_collections_ordered()
    &&& match (command, outcome) {
        (
            SchedulerCommandKind::AcknowledgeCancellation { .. },
            AcknowledgeCancellationOutcome::NotAcknowledgeCancellationCommand,
        ) => false,
        (_, AcknowledgeCancellationOutcome::NotAcknowledgeCancellationCommand) => {
            *after == *before
        },
        (
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id },
            AcknowledgeCancellationOutcome::DispatchNotActive,
        ) => {
            &&& forall |index: int| #![trigger before.spec_reservations()[index]]
                0 <= index < before.spec_reservations().len() ==>
                    before.spec_reservations()[index].spec_dispatch_id() != *dispatch_id
            &&& *after == *before
        },
        (
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id },
            AcknowledgeCancellationOutcome::ReservationWorkDisappeared,
        ) => {
            &&& exists |reservation_index: int|
                reservation_at(before, reservation_index, *dispatch_id)
                    && forall |work_index: int| #![trigger before.spec_work()[work_index]]
                        0 <= work_index < before.spec_work().len() ==>
                            before.spec_work()[work_index].spec_definition().spec_id()
                                != before.spec_reservations()[reservation_index].spec_work_id()
            &&& *after == *before
        },
        (
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id },
            AcknowledgeCancellationOutcome::WorkNotCancelling,
        ) => {
            &&& exists |reservation_index: int, work_index: int|
                reservation_at(before, reservation_index, *dispatch_id)
                    && work_at(
                        before,
                        work_index,
                        before.spec_reservations()[reservation_index].spec_work_id(),
                    )
                    && before.spec_work()[work_index].spec_phase() != WorkPhase::Cancelling
            &&& *after == *before
        },
        (
            SchedulerCommandKind::AcknowledgeCancellation { .. },
            AcknowledgeCancellationOutcome::CancellingWorkDisappeared,
        ) => false,
        (
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id },
            AcknowledgeCancellationOutcome::Applied,
        ) => exists |reservation_index: int, work_index: int, removed: SchedulerReservation|
            reservation_at(before, reservation_index, *dispatch_id)
                && work_at(
                    before,
                    work_index,
                    before.spec_reservations()[reservation_index].spec_work_id(),
                )
                && before.spec_work()[work_index].spec_phase() == WorkPhase::Cancelling
                && super::super::terminal_release_matches(
                    before,
                    after,
                    *dispatch_id,
                    before.spec_reservations()[reservation_index].spec_work_id(),
                    WorkTerminal::Cancelled,
                    &removed,
                ),
        (_, _) => false,
    }
}

pub(super) proof fn establish_complete_not_acknowledged(
    before: &SchedulerState,
    command: &SchedulerCommandKind,
    dispatch_id: DispatchId,
    reservation_index: int,
    work_index: int,
)
    requires
        match command {
            SchedulerCommandKind::CompleteWork { dispatch_id: requested, .. } => {
                *requested == dispatch_id
            },
            _ => false,
        },
        reservation_at(before, reservation_index, dispatch_id),
        work_at(
            before,
            work_index,
            before.spec_reservations()[reservation_index].spec_work_id(),
        ),
        !before.spec_reservations()[reservation_index].spec_started()
            || before.spec_work()[work_index].spec_phase() != WorkPhase::Running,
    ensures complete_command_outcome_matches(
        before,
        before,
        command,
        CompleteCommandOutcome::NotAcknowledgedRunning,
    ),
{
    reveal(complete_command_outcome_matches);
    assert(exists |target_reservation: int, target_work: int|
        reservation_at(before, target_reservation, dispatch_id)
            && work_at(
                before, target_work,
                before.spec_reservations()[target_reservation].spec_work_id(),
            )
            && (!before.spec_reservations()[target_reservation].spec_started()
                || before.spec_work()[target_work].spec_phase() != WorkPhase::Running)) by {
        assert(reservation_at(before, reservation_index, dispatch_id));
        assert(work_at(
            before,
            work_index,
            before.spec_reservations()[reservation_index].spec_work_id(),
        ));
    }
}

pub(super) proof fn establish_work_not_cancelling(
    before: &SchedulerState,
    command: &SchedulerCommandKind,
    dispatch_id: DispatchId,
    reservation_index: int,
    work_index: int,
)
    requires
        match command {
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: requested } => {
                *requested == dispatch_id
            },
            _ => false,
        },
        reservation_at(before, reservation_index, dispatch_id),
        work_at(
            before,
            work_index,
            before.spec_reservations()[reservation_index].spec_work_id(),
        ),
        before.spec_work()[work_index].spec_phase() != WorkPhase::Cancelling,
    ensures acknowledge_cancellation_outcome_matches(
        before,
        before,
        command,
        AcknowledgeCancellationOutcome::WorkNotCancelling,
    ),
{
    reveal(acknowledge_cancellation_outcome_matches);
    assert(exists |target_reservation: int, target_work: int|
        reservation_at(before, target_reservation, dispatch_id)
            && work_at(
                before, target_work,
                before.spec_reservations()[target_reservation].spec_work_id(),
            )
            && before.spec_work()[target_work].spec_phase() != WorkPhase::Cancelling) by {
        assert(reservation_at(before, reservation_index, dispatch_id));
        assert(work_at(
            before,
            work_index,
            before.spec_reservations()[reservation_index].spec_work_id(),
        ));
    }
}

} // verus!
