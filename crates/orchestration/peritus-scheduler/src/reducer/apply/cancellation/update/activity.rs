//! Reservation membership scan used by one cancellation update.

use crate::{SchedulerReservation, WorkId};

pub(super) fn is_active(reservations: &[SchedulerReservation], id: WorkId) -> bool {
    let mut index = 0;
    while index < reservations.len() {
        if reservations[index].work_id().same(&id) {
            return true;
        }
        index += 1;
    }
    false
}
