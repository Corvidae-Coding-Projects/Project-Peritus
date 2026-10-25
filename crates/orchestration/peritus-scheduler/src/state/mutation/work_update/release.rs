//! Combined reservation release and work lifecycle transitions.

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkTerminal};
use peritus_types::Sha256Digest;

use super::{WorkUpdate, update_work};

fn release_with_update(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    update: WorkUpdate,
) -> Option<SchedulerReservation> {
    let reservation = super::super::remove_reservation(state, dispatch_id)?;
    if !update_work(state, work_id, update) {
        return None;
    }
    Some(reservation)
}
pub fn release_to_phase(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    phase: WorkPhase,
) -> Option<SchedulerReservation> {
    release_with_update(state, dispatch_id, work_id, WorkUpdate::Phase(phase))
}
pub fn release_to_retry_pending(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    cause: Sha256Digest,
) -> Option<SchedulerReservation> {
    release_with_update(state, dispatch_id, work_id, WorkUpdate::RetryPending(cause))
}
pub fn release_to_terminal(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    terminal: WorkTerminal,
) -> Option<SchedulerReservation> {
    release_with_update(state, dispatch_id, work_id, WorkUpdate::Terminal(terminal))
}
