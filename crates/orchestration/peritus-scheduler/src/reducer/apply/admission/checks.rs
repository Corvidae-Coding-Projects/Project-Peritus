//! Exact bounded scans used by production work admission.

use crate::{SchedulerState, WorkId, WorkSpec, WorkerPhase, WorkerRecord};

pub fn contains_work(state: &SchedulerState, id: WorkId) -> bool {
    state.work_item(id).is_some()
}
pub fn all_dependencies_retained(state: &SchedulerState, spec: &WorkSpec) -> bool {
    let dependencies = spec.dependencies();
    let mut index = 0;
    while index < dependencies.len() {
        if !contains_work(state, dependencies[index]) {
            return false;
        }
        index += 1;
    }
    true
}
pub fn has_retained_parent(state: &SchedulerState, spec: &WorkSpec) -> bool {
    let Some(parent) = spec.parent() else {
        return true;
    };
    contains_work(state, parent)
}
fn eligible_worker(worker: &WorkerRecord, spec: &WorkSpec) -> bool {
    !worker.phase().same(WorkerPhase::Removed)
        && crate::identity::actor_ids_same(worker.descriptor().owner(), spec.owner())
        && worker.descriptor().supports(spec.class())
        && spec.request().fits_within(worker.descriptor().capacity())
}
pub fn has_supporting_worker(state: &SchedulerState, spec: &WorkSpec) -> bool {
    let workers = state.workers();
    let mut index = 0;
    while index < workers.len() {
        if eligible_worker(&workers[index], spec) {
            return true;
        }
        index += 1;
    }
    false
}
