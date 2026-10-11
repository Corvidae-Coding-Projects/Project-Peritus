//! Atomic reducer mutation for dispatch start acknowledgement.

use crate::{DispatchId, SchedulerState, WorkId, WorkPhase};

use super::work_update::{WorkUpdate, update_work};

#[doc = " Marks the reservation started, then moves its work to running."]
#[doc = ""]
#[doc = " `None` retains the legacy dispatch-disappeared result. `Some(false)` retains"]
#[doc = " the legacy work-disappeared result after the start bit was updated."]
pub fn acknowledge_reservation_start(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
) -> Option<bool> {
    if !super::mark_reservation_started(state, dispatch_id) {
        return None;
    }
    if !update_work(state, work_id, WorkUpdate::Phase(WorkPhase::Running)) {
        return Some(false);
    }
    Some(true)
}
