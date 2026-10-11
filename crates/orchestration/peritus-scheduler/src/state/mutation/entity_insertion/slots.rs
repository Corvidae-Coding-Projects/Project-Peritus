//! Exact canonical insertion positions from the production binary-search algorithm.

use crate::{WorkId, WorkRecord, WorkerId, WorkerRecord};

pub(super) fn worker_slot(values: &[WorkerRecord], id: WorkerId) -> usize {
    let mut size = values.len();
    if size == 0 {
        return 0;
    }
    let mut base: usize = 0;
    while size > 1 {
        let half = size / 2;
        let mid = base + half;
        let observed = values[mid].descriptor().id();
        if !id.precedes(&observed) {
            base = mid;
        }
        size -= half;
    }
    let observed = values[base].descriptor().id();
    if observed.precedes(&id) { base + 1 } else { base }
}
pub(super) fn work_slot(values: &[WorkRecord], id: WorkId) -> usize {
    let mut size = values.len();
    if size == 0 {
        return 0;
    }
    let mut base: usize = 0;
    while size > 1 {
        let half = size / 2;
        let mid = base + half;
        let observed = values[mid].spec().id();
        if !id.precedes(&observed) {
            base = mid;
        }
        size -= half;
    }
    let observed = values[base].spec().id();
    if observed.precedes(&id) { base + 1 } else { base }
}
