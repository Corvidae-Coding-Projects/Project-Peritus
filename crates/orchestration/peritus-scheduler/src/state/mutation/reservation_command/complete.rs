//! Successful completion through the exact reservation release kernel.

use super::CompleteCommandOutcome;

use crate::{SchedulerCommandKind, SchedulerState, WorkPhase, WorkTerminal};

#[doc = " Applies the actual `CompleteWork` payload through the verified release mutation."]
pub fn apply_complete_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> CompleteCommandOutcome {
    let SchedulerCommandKind::CompleteWork {
        dispatch_id: requested_dispatch,
        result_digest: supplied_result,
    } = command
    else {
        return CompleteCommandOutcome::NotCompleteCommand;
    };
    let (dispatch_id, result_digest) = (*requested_dispatch, *supplied_result);
    let Some(reservation) = state.reservation(dispatch_id) else {
        return CompleteCommandOutcome::DispatchNotActive;
    };
    let work_id = reservation.work_id();
    let started = reservation.started();
    let Some(work) = state.work_item(work_id) else {
        return CompleteCommandOutcome::WorkDisappeared;
    };
    let phase = work.phase();
    if !started || !phase.same(WorkPhase::Running) {
        return CompleteCommandOutcome::NotAcknowledgedRunning;
    }
    let Some(_removed) = super::super::release_to_terminal(
        state,
        dispatch_id,
        work_id,
        WorkTerminal::Succeeded { result_digest },
    ) else {
        return CompleteCommandOutcome::WorkDisappeared;
    };
    CompleteCommandOutcome::Applied
}
