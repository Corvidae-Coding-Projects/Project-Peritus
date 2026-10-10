//! Exact feasibility scans used by deterministic selection.

use crate::{SchedulerState, WorkPhase, WorkerPhase};

use super::capacity;

mod work;

pub(super) use work::select_next_indexed;

fn worker_is_feasible_for(state: &SchedulerState, work_index: usize, worker_index: usize) -> bool {
    let work = &state.work()[work_index];
    let worker = &state.workers()[worker_index];
    let worker_id = worker.descriptor().id();
    if !worker.phase().same(WorkerPhase::Available)
        || !crate::identity::actor_ids_same(worker.descriptor().owner(), work.spec().owner())
        || !worker.descriptor().supports(work.spec().class())
    {
        return false;
    }
    let count = super::worker_reservation_count(state, worker_id);
    if count >= worker.descriptor().concurrency() as usize {
        return false;
    }
    let worker_fits = capacity::worker_fits_after(
        state,
        worker_id,
        worker.descriptor().capacity(),
        work.spec().request(),
    );
    if !worker_fits {
        return false;
    }
    true
}
pub(super) fn first_feasible_worker(state: &SchedulerState, work_index: usize) -> Option<usize> {
    let work_records = state.work();
    let work = &work_records[work_index];
    if !work.phase().same(WorkPhase::Queued) {
        return None;
    }
    let global_fits = capacity::global_fits_after(state, work.spec().request());
    if !global_fits {
        return None;
    }
    let workers = state.workers();
    let mut worker_index = 0;
    while worker_index < workers.len() {
        if worker_is_feasible_for(state, work_index, worker_index) {
            return Some(worker_index);
        }
        worker_index += 1;
    }
    None
}
