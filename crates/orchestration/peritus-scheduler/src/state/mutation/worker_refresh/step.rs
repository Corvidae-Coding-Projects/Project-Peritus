//! Exact effect of refreshing one worker from the retained identity snapshot.

use crate::state::mutation;
use crate::{SchedulerState, WorkerId, WorkerPhase};

#[doc = " Refreshes one retained identity with the same lookup and mutation order as production."]
pub(super) fn refresh_one_worker(state: &mut SchedulerState, id: WorkerId, _index: usize) {
    let Some(worker) = state.worker(id) else {
        return;
    };
    let phase = worker.phase();
    if matches!(phase, WorkerPhase::Draining | WorkerPhase::Lost | WorkerPhase::Removed) {
        return;
    }
    let concurrency = worker.descriptor().concurrency();
    let active = crate::selection::worker_reservation_count(state, id);
    let target =
        if active >= usize::from(concurrency) { WorkerPhase::Busy } else { WorkerPhase::Available };
    let _found = mutation::set_worker_phase(state, id, target);
}
