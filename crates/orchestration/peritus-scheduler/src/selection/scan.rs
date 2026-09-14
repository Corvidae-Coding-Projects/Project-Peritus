//! Exact feasibility scans used by deterministic selection.

use vstd::prelude::*;

use crate::{SchedulerState, WorkPhase, WorkerPhase};

use super::capacity;
#[cfg(verus_only)]
use super::model::{
    dispatch_candidate, exact_selection_ready, first_candidate_worker, selected_pair_admitted,
    selected_pair_feasible, worker_candidate,
};

mod work;

pub(super) use work::select_next_indexed;

verus! {

proof fn establish_selected_pair(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
)
    requires
        0 <= work_index < state.spec_work().len(),
        0 <= worker_index < state.spec_workers().len(),
        state.spec_work()[work_index].spec_phase() == WorkPhase::Queued,
        state.spec_workers()[worker_index].spec_phase() == WorkerPhase::Available,
        crate::identity::actor_ids_match(
            state.spec_workers()[worker_index].spec_descriptor().spec_owner(),
            state.spec_work()[work_index].spec_definition().spec_owner(),
        ),
        state.spec_workers()[worker_index].spec_descriptor().spec_classes().contains(
            state.spec_work()[work_index].spec_definition().spec_class(),
        ),
        crate::verified::worker_count(
            state.spec_reservations(),
            state.spec_workers()[worker_index].spec_descriptor().spec_id(),
        ) < state.spec_workers()[worker_index].spec_descriptor().spec_concurrency(),
        forall |kind: crate::ResourceKind| #![auto]
            crate::verified::reservation_quantity(state.spec_reservations(), kind)
                    + crate::verified::vector_quantity(
                        state.spec_work()[work_index]
                            .spec_definition().spec_request().spec_entries(),
                        kind,
                    ) <= state.spec_binding().spec_capacity().spec_quantity(kind),
        forall |kind: crate::ResourceKind| #![auto]
            crate::verified::worker_quantity(
                state.spec_reservations(),
                state.spec_workers()[worker_index].spec_descriptor().spec_id(),
                kind,
            ) + crate::verified::vector_quantity(
                state.spec_work()[work_index]
                    .spec_definition().spec_request().spec_entries(),
                kind,
            ) <= state.spec_workers()[worker_index]
                .spec_descriptor().spec_capacity().spec_quantity(kind),
    ensures selected_pair_admitted(state, work_index, worker_index),
{
    reveal(selected_pair_admitted);
    reveal(selected_pair_feasible);
}

fn worker_is_feasible_for(
    state: &SchedulerState,
    work_index: usize,
    worker_index: usize,
) -> (result: bool)
    requires
        work_index < state.spec_work().len(),
        worker_index < state.spec_workers().len(),
        state.spec_work()[work_index as int].spec_phase() == WorkPhase::Queued,
        state.spec_reservation_invariant() ==> forall |kind: crate::ResourceKind| #![auto]
            crate::verified::reservation_quantity(state.spec_reservations(), kind)
                    + crate::verified::vector_quantity(
                        state.spec_work()[work_index as int]
                            .spec_definition().spec_request().spec_entries(),
                        kind,
                    ) <= state.spec_binding().spec_capacity().spec_quantity(kind),
    ensures
        exact_selection_ready(state) ==>
            result == worker_candidate(state, work_index as int, worker_index as int),
        result && state.spec_reservation_invariant() ==>
            selected_pair_admitted(state, work_index as int, worker_index as int),
{
    let work = &state.work()[work_index];
    let worker = &state.workers()[worker_index];
    let worker_id = worker.descriptor().id();
    if !worker.phase().same(WorkerPhase::Available)
        || !crate::identity::actor_ids_same(worker.descriptor().owner(), work.spec().owner())
        || !worker.descriptor().supports(work.spec().class())
    {
        proof {
            if exact_selection_ready(state) {
                reveal(exact_selection_ready);
                reveal(worker_candidate);
            }
        }
        return false;
    }
    let count = super::worker_reservation_count(state, worker_id);
    if count >= worker.descriptor().concurrency() as usize {
        proof {
            if exact_selection_ready(state) {
                reveal(exact_selection_ready);
                reveal(worker_candidate);
            }
        }
        return false;
    }
    let worker_fits = capacity::worker_fits_after(
        state,
        worker_id,
        worker.descriptor().capacity(),
        work.spec().request(),
    );
    if !worker_fits {
        proof {
            if exact_selection_ready(state) {
                reveal(exact_selection_ready);
                reveal(worker_candidate);
            }
        }
        return false;
    }
    proof {
        if state.spec_reservation_invariant() {
            capacity::worker_entrywise_implies_all(
                state,
                worker_index as int,
                work.spec_definition().spec_request(),
            );
            assert(count as int == crate::verified::worker_count(
                state.spec_reservations(),
                worker.spec_descriptor().spec_id(),
            ));
            establish_selected_pair(state, work_index as int, worker_index as int);
        }
        if exact_selection_ready(state) {
            reveal(exact_selection_ready);
            reveal(worker_candidate);
        }
    }
    true
}

pub(super) fn first_feasible_worker(
    state: &SchedulerState,
    work_index: usize,
) -> (result: Option<usize>)
    requires work_index < state.spec_work().len(),
    ensures
        match result {
            Some(worker_index) =>
                worker_index < state.spec_workers().len()
                    && (state.spec_reservation_invariant() ==> selected_pair_admitted(
                        state,
                        work_index as int,
                        worker_index as int,
                    )),
            None => true,
        },
        exact_selection_ready(state) ==> match result {
            Some(worker_index) => first_candidate_worker(
                state, work_index as int, worker_index as int,
            ),
            None => forall |worker_index: int|
                0 <= worker_index < state.spec_workers().len() ==>
                    !dispatch_candidate(state, work_index as int, worker_index),
        },
{
    let work_records = state.work();
    let work = &work_records[work_index];
    proof {
        assert(work_records@ == state.spec_work());
        assert(*work == state.spec_work()[work_index as int]);
    }
    if !work.phase().same(WorkPhase::Queued) {
        proof {
            if exact_selection_ready(state) {
                reveal(dispatch_candidate);
            }
        }
        return None;
    }
    let global_fits = capacity::global_fits_after(state, work.spec().request());
    if !global_fits {
        proof {
            if exact_selection_ready(state) {
                reveal(exact_selection_ready);
                reveal(dispatch_candidate);
            }
        }
        return None;
    }
    proof {
        if state.spec_reservation_invariant() {
            capacity::global_entrywise_implies_all(
                state,
                work.spec_definition().spec_request(),
            );
        }
    }
    let workers = state.workers();
    let mut worker_index = 0;
    while worker_index < workers.len()
        invariant
            work_index < state.spec_work().len(),
            work_records@ == state.spec_work(),
            *work == state.spec_work()[work_index as int],
            work.spec_phase() == WorkPhase::Queued,
            workers@ == state.spec_workers(),
            worker_index <= state.spec_workers().len(),
            state.spec_reservation_invariant() ==> forall |kind: crate::ResourceKind| #![auto]
                crate::verified::reservation_quantity(
                    state.spec_reservations(), kind,
                ) + crate::verified::vector_quantity(
                    work.spec_definition().spec_request().spec_entries(), kind,
                ) <= state.spec_binding().spec_capacity().spec_quantity(kind),
            exact_selection_ready(state) ==> capacity::global_entries_fit_after(
                state,
                work.spec_definition().spec_request().spec_entries(),
            ),
            exact_selection_ready(state) ==> forall |prior: int|
                0 <= prior < worker_index ==>
                    !dispatch_candidate(state, work_index as int, prior),
        decreases state.spec_workers().len() - worker_index,
    {
        if worker_is_feasible_for(state, work_index, worker_index) {
            proof {
                if exact_selection_ready(state) {
                    reveal(first_candidate_worker);
                    reveal(dispatch_candidate);
                }
            }
            return Some(worker_index);
        }
        proof {
            if exact_selection_ready(state) {
                assert(!worker_candidate(
                    state, work_index as int, worker_index as int,
                ));
                reveal(dispatch_candidate);
            }
        }
        worker_index += 1;
    }
    None
}

} // verus!
