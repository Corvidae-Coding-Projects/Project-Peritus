//! Verified reservation bookkeeping mutations.

use crate::{DispatchId, SchedulerState};

fn mark_reservation_started_at(state: &mut SchedulerState, index: usize, _id: DispatchId) {
    state.reservations[index].mark_started();
}
#[doc = " Marks one live reservation started without changing capacity or ownership."]
pub fn mark_reservation_started(state: &mut SchedulerState, id: DispatchId) -> bool {
    let mut index = 0;
    while index < state.reservations.len() {
        if state.reservations[index].dispatch_id().same(&id) {
            mark_reservation_started_at(state, index, id);
            return true;
        }
        index += 1;
    }
    false
}
