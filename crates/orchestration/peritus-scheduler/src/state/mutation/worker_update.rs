//! Verified reservation-stable worker lifecycle mutations.

use crate::{SchedulerState, WorkerId, WorkerPhase};

fn set_worker_phase_at(
    state: &mut SchedulerState,
    index: usize,
    _id: WorkerId,
    phase: WorkerPhase,
) {
    state.workers[index].set_phase(phase);
}
fn scan_and_set_worker_phase(state: &mut SchedulerState, id: WorkerId, phase: WorkerPhase) -> bool {
    let mut index = 0;
    while index < state.workers.len() {
        if state.workers[index].descriptor().id().same(&id) {
            set_worker_phase_at(state, index, id, phase);
            return true;
        }
        index += 1;
    }
    false
}
pub fn set_worker_phase(state: &mut SchedulerState, id: WorkerId, phase: WorkerPhase) -> bool {
    if let Some(index) = state.worker_index(id) {
        set_worker_phase_at(state, index, id, phase);
        true
    } else {
        scan_and_set_worker_phase(state, id, phase)
    }
}
