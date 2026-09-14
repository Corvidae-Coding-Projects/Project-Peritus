//! Exact worker lookup and final lifecycle-target facts for loss application.

use vstd::prelude::*;

use crate::{SchedulerState, WorkerId, WorkerPhase};

use super::WorkerLossError;
#[cfg(verus_only)]
use super::worker_loss_target;

verus! {

pub(super) fn validate_worker_target(
    state: &SchedulerState,
    worker_id: WorkerId,
) -> (result: Result<(), WorkerLossError>)
    ensures
        state.spec_collections_ordered() && worker_loss_target(state, worker_id)
            ==> result.is_ok(),
{
    let Some(worker) = state.worker(worker_id) else {
        proof {
            if state.spec_collections_ordered() && worker_loss_target(state, worker_id) {
                reveal(SchedulerState::spec_collections_ordered);
                missing_worker_is_impossible(state, worker_id);
            }
        }
        return Err(WorkerLossError::WorkerMissing);
    };
    let phase = worker.phase();
    if matches!(phase, WorkerPhase::Lost | WorkerPhase::Removed) {
        proof {
            if state.spec_collections_ordered() && worker_loss_target(state, worker_id) {
                inactive_worker_is_impossible(state, worker_id, phase);
            }
        }
        return Err(WorkerLossError::AlreadyLostOrRemoved);
    }
    Ok(())
}

proof fn matching_worker_indices_are_equal(
    workers: Seq<crate::WorkerRecord>,
    left: int,
    right: int,
)
    requires
        SchedulerState::worker_records_ordered(workers),
        0 <= left < workers.len(),
        0 <= right < workers.len(),
        workers[left].spec_descriptor().spec_id()
            == workers[right].spec_descriptor().spec_id(),
    ensures left == right,
{
    let id = workers[left].spec_descriptor().spec_id();
    if left < right {
        assert(id.spec_precedes(&workers[right].spec_descriptor().spec_id()));
        WorkerId::order_irreflexive(&id);
    } else if right < left {
        assert(workers[right].spec_descriptor().spec_id().spec_precedes(&id));
        WorkerId::order_irreflexive(&id);
    }
}

pub(super) proof fn missing_worker_is_impossible(
    state: &SchedulerState,
    worker_id: WorkerId,
)
    requires
        state.spec_collections_ordered(),
        worker_loss_target(state, worker_id),
        forall |index: int| 0 <= index < state.spec_workers().len() ==>
            state.spec_workers()[index].spec_descriptor().spec_id() != worker_id,
    ensures false,
{
    reveal(worker_loss_target);
}

pub(super) proof fn inactive_worker_is_impossible(
    state: &SchedulerState,
    worker_id: WorkerId,
    observed_phase: WorkerPhase,
)
    requires
        state.spec_collections_ordered(),
        worker_loss_target(state, worker_id),
        exists |observed: int| #![trigger state.spec_workers()[observed]] {
            &&& 0 <= observed < state.spec_workers().len()
            &&& state.spec_workers()[observed].spec_descriptor().spec_id() == worker_id
            &&& state.spec_workers()[observed].spec_phase() == observed_phase
        },
        observed_phase == WorkerPhase::Lost || observed_phase == WorkerPhase::Removed,
    ensures false,
{
    reveal(worker_loss_target);
    reveal(SchedulerState::spec_collections_ordered);
    reveal(SchedulerState::spec_workers_ordered);
    let target = choose |index: int| #![trigger state.spec_workers()[index]] {
        &&& 0 <= index < state.spec_workers().len()
        &&& state.spec_workers()[index].spec_descriptor().spec_id() == worker_id
        &&& state.spec_workers()[index].spec_phase() != WorkerPhase::Lost
        &&& state.spec_workers()[index].spec_phase() != WorkerPhase::Removed
    };
    let observed = choose |index: int| #![trigger state.spec_workers()[index]] {
        &&& 0 <= index < state.spec_workers().len()
        &&& state.spec_workers()[index].spec_descriptor().spec_id() == worker_id
        &&& state.spec_workers()[index].spec_phase() == observed_phase
    };
    matching_worker_indices_are_equal(state.spec_workers(), target, observed);
}

pub(super) proof fn lost_worker_disappearance_is_impossible(
    initial: &SchedulerState,
    released: &SchedulerState,
    after: &SchedulerState,
    worker_id: WorkerId,
)
    requires
        worker_loss_target(initial, worker_id),
        released.spec_workers() == initial.spec_workers(),
        crate::state::mutation::worker_phase_state_matches(
            released, after, worker_id, WorkerPhase::Lost, false,
        ),
    ensures false,
{
    reveal(worker_loss_target);
    reveal(crate::state::mutation::worker_phase_state_matches);
    reveal(crate::state::mutation::worker_phase_update_matches);
}

} // verus!
