//! Cancellation acknowledgement through the exact reservation release kernel.

use super::AcknowledgeCancellationOutcome;

use crate::{SchedulerCommandKind, SchedulerState, WorkPhase, WorkTerminal};

#[doc = " Applies the actual `AcknowledgeCancellation` payload through the verified release mutation."]
pub fn apply_acknowledge_cancellation_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> AcknowledgeCancellationOutcome {
    let SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: requested_dispatch } = command
    else {
        return AcknowledgeCancellationOutcome::NotAcknowledgeCancellationCommand;
    };
    let dispatch_id = *requested_dispatch;
    let Some(reservation) = state.reservation(dispatch_id) else {
        return AcknowledgeCancellationOutcome::DispatchNotActive;
    };
    let work_id = reservation.work_id();
    let Some(work) = state.work_item(work_id) else {
        return AcknowledgeCancellationOutcome::ReservationWorkDisappeared;
    };
    let phase = work.phase();
    if !phase.same(WorkPhase::Cancelling) {
        return AcknowledgeCancellationOutcome::WorkNotCancelling;
    }
    let Some(_removed) =
        super::super::release_to_terminal(state, dispatch_id, work_id, WorkTerminal::Cancelled)
    else {
        return AcknowledgeCancellationOutcome::CancellingWorkDisappeared;
    };
    AcknowledgeCancellationOutcome::Applied
}
