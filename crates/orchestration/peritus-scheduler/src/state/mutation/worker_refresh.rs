//! Exact worker availability refresh after reservation mutations.

mod step;

use crate::{SchedulerState, WorkerId, WorkerRecord};
use step::refresh_one_worker;

fn retained_worker_ids(workers: &[WorkerRecord]) -> Vec<WorkerId> {
    let mut ids = Vec::new();
    let mut index = 0;
    while index < workers.len() {
        ids.push(workers[index].descriptor().id());
        index += 1;
    }
    ids
}
#[doc = " Refreshes every mutable worker against the exact retained reservation count."]
pub(super) fn refresh_worker_phases(state: &mut SchedulerState) {
    let ids = retained_worker_ids(state.workers());
    let mut index = 0;
    while index < ids.len() {
        let id = ids[index];
        refresh_one_worker(state, id, index);
        index += 1;
    }
}
