//! Exact live-reservation removal used by combined release transitions.

use crate::{DispatchId, SchedulerReservation, SchedulerState};

pub fn remove_reservation(
    state: &mut SchedulerState,
    id: DispatchId,
) -> Option<SchedulerReservation> {
    let mut index = 0;
    while index < state.reservations.len() {
        if state.reservations[index].dispatch_id().same(&id) {
            let removed = state.reservations.remove(index);
            return Some(removed);
        }
        index += 1;
    }
    None
}
