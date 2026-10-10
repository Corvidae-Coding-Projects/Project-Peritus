//! Production worker and work insertion with actual-state preservation contracts.

use crate::{SchedulerState, WorkRecord, WorkerRecord};

mod slots;
use slots::{work_slot, worker_slot};

#[doc = " Inserts one worker at the same canonical binary-search position used by reducer admission."]
pub fn insert_worker(state: &mut SchedulerState, value: WorkerRecord) {
    let at = worker_slot(&state.workers, value.descriptor().id());
    state.workers.insert(at, value);
}
#[doc = " Inserts one work record at the same canonical binary-search position used by admission."]
pub fn insert_work(state: &mut SchedulerState, value: WorkRecord) {
    let at = work_slot(&state.work, value.spec().id());
    state.work.insert(at, value);
}
