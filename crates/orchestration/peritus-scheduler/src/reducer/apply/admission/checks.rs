//! Exact bounded scans used by production work admission.

use crate::{SchedulerState, WorkId, WorkSpec, WorkerPhase, WorkerRecord};
use vstd::prelude::*;

verus! {

pub open spec fn retained(state: &SchedulerState, id: WorkId) -> bool {
    exists |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id() == id
}

pub fn contains_work(state: &SchedulerState, id: WorkId) -> (found: bool)
    ensures state.spec_work_ordered() ==> found == retained(state, id),
{
    let found = state.work_item(id).is_some();
    proof { reveal(retained); }
    found
}

pub open spec fn dependencies_retained(state: &SchedulerState, spec: &WorkSpec) -> bool {
    forall |index: int| #![trigger spec.spec_dependencies()[index]]
        0 <= index < spec.spec_dependencies().len() ==>
            retained(state, spec.spec_dependencies()[index])
}

pub fn all_dependencies_retained(state: &SchedulerState, spec: &WorkSpec) -> (found: bool)
    ensures state.spec_work_ordered() ==> found == dependencies_retained(state, spec),
{
    let dependencies = spec.dependencies();
    let mut index = 0;
    while index < dependencies.len()
        invariant
            index <= dependencies.len(),
            dependencies@ == spec.spec_dependencies(),
            state.spec_work_ordered() ==> forall |prior: int|
                #![trigger dependencies@[prior]] 0 <= prior < index ==>
                    retained(state, dependencies@[prior]),
        decreases dependencies.len() - index,
    {
        if !contains_work(state, dependencies[index]) {
            proof { reveal(dependencies_retained); }
            return false;
        }
        index += 1;
    }
    proof { reveal(dependencies_retained); }
    true
}

pub open spec fn parent_retained(state: &SchedulerState, spec: &WorkSpec) -> bool {
    match spec.spec_parent() {
        Some(parent) => retained(state, parent),
        None => true,
    }
}

pub fn has_retained_parent(state: &SchedulerState, spec: &WorkSpec) -> (found: bool)
    ensures state.spec_work_ordered() ==> found == parent_retained(state, spec),
{
    let Some(parent) = spec.parent() else { return true; };
    contains_work(state, parent)
}

pub open spec fn worker_supports(worker: &WorkerRecord, spec: &WorkSpec) -> bool {
    &&& worker.spec_phase() != WorkerPhase::Removed
    &&& crate::identity::actor_ids_match(
        worker.spec_descriptor().spec_owner(), spec.spec_owner(),
    )
    &&& worker.spec_descriptor().spec_classes().contains(spec.spec_class())
    &&& spec.spec_request().spec_fits_within(worker.spec_descriptor().spec_capacity())
}

fn eligible_worker(worker: &WorkerRecord, spec: &WorkSpec) -> (eligible: bool)
    ensures eligible == worker_supports(worker, spec),
{
    !worker.phase().same(WorkerPhase::Removed)
        && crate::identity::actor_ids_same(worker.descriptor().owner(), spec.owner())
        && worker.descriptor().supports(spec.class())
        && spec.request().fits_within(worker.descriptor().capacity())
}

pub open spec fn supporting_worker_exists(state: &SchedulerState, spec: &WorkSpec) -> bool {
    exists |index: int| #![trigger state.spec_workers()[index]]
        0 <= index < state.spec_workers().len()
            && worker_supports(&state.spec_workers()[index], spec)
}

pub fn has_supporting_worker(state: &SchedulerState, spec: &WorkSpec) -> (found: bool)
    ensures found == supporting_worker_exists(state, spec),
{
    let workers = state.workers();
    let mut index = 0;
    while index < workers.len()
        invariant
            index <= workers.len(),
            workers@ == state.spec_workers(),
            forall |prior: int| #![trigger workers@[prior]] 0 <= prior < index ==>
                !worker_supports(&workers@[prior], spec),
        decreases workers.len() - index,
    {
        if eligible_worker(&workers[index], spec) {
            proof { reveal(supporting_worker_exists); }
            return true;
        }
        index += 1;
    }
    proof { reveal(supporting_worker_exists); }
    false
}

} // verus!
