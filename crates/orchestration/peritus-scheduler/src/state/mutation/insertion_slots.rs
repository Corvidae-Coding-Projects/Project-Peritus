//! Exact insertion positions for live and historical dispatch identities.

use crate::{DispatchId, SchedulerReservation};

pub(super) fn reservation_slot(values: &[SchedulerReservation], id: DispatchId) -> usize {
    let mut size = values.len();
    if size == 0 {
        return 0;
    }
    let mut base: usize = 0;
    while size > 1 {
        let half = size / 2;
        let mid = base + half;
        let observed = values[mid].dispatch_id();
        if !id.precedes(&observed) {
            base = mid;
        }
        size -= half;
    }
    let observed = values[base].dispatch_id();
    if observed.precedes(&id) { base + 1 } else { base }
}
pub(super) fn dispatch_slot(values: &[DispatchId], id: DispatchId) -> usize {
    let mut index = 0;
    while index < values.len() {
        if id.precedes(&values[index]) {
            return index;
        }
        index += 1;
    }
    index
}
