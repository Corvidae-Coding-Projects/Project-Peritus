//! Pure deterministic bounded-bypass selection.

use vstd::prelude::*;

use crate::{SchedulerState, WorkId, WorkPhase, WorkerPhase};

mod capacity;
mod model;

pub use capacity::worker_reservation_count;

use model::IndexedSelection;
pub use model::Selection;

#[cfg(verus_only)]
use model::selection_is_feasible;
#[cfg(verus_only)]
pub(crate) use model::{
    admitted_work_is_unreserved, selected_pair_admitted, selected_pair_feasible,
    selected_pair_feasible_parts,
};

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
        return false;
    }
    let count = worker_reservation_count(state, worker_id);
    if count >= worker.descriptor().concurrency() as usize
        || !capacity::worker_fits_after(
            state,
            worker_id,
            worker.descriptor().capacity(),
            work.spec().request(),
        )
    {
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
    }
    true
}

fn first_feasible_worker(
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
{
    let work_records = state.work();
    let work = &work_records[work_index];
    proof {
        assert(work_records@ == state.spec_work());
        assert(*work == state.spec_work()[work_index as int]);
    }
    if !work.phase().same(WorkPhase::Queued)
        || !capacity::global_fits_after(state, work.spec().request())
    {
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
        decreases state.spec_workers().len() - worker_index,
    {
        if worker_is_feasible_for(state, work_index, worker_index) {
            return Some(worker_index);
        }
        worker_index += 1;
    }
    None
}

fn candidate_precedes(
    state: &SchedulerState,
    left_index: usize,
    right_index: usize,
) -> (result: bool)
    requires
        left_index < state.spec_work().len(),
        right_index < state.spec_work().len(),
{
    let left = &state.work()[left_index];
    let right = &state.work()[right_index];
    let limit = state.binding().limits().bypass_count();
    let left_aged = left.bypasses() >= limit;
    let right_aged = right.bypasses() >= limit;
    if left_aged != right_aged {
        left_aged
    } else if left.spec().priority() != right.spec().priority() {
        left.spec().priority() > right.spec().priority()
    } else if left.enqueue_ordinal() != right.enqueue_ordinal() {
        left.enqueue_ordinal() < right.enqueue_ordinal()
    } else {
        left.spec().id().precedes(&right.spec().id())
    }
}

/// Selects the next feasible item by aged-first, priority, enqueue ordinal, identity, then worker.
#[must_use]
fn select_next_indexed(state: &SchedulerState) -> (result: Option<IndexedSelection>)
    ensures
        match result {
            Some(selection) =>
                selection.work_index < state.spec_work().len()
                    && selection.worker_index < state.spec_workers().len()
                    && (state.spec_reservation_invariant() ==> selected_pair_admitted(
                        state,
                        selection.work_index as int,
                        selection.worker_index as int,
                    )),
            None => true,
        },
{
    let mut chosen: Option<(usize, usize)> = None;
    let mut work_index = 0;
    while work_index < state.work().len()
        invariant
            work_index <= state.spec_work().len(),
            match chosen {
                Some((selected_work, selected_worker)) =>
                    selected_work < state.spec_work().len()
                        && selected_worker < state.spec_workers().len()
                        && (state.spec_reservation_invariant() ==> selected_pair_admitted(
                            state,
                            selected_work as int,
                            selected_worker as int,
                        )),
                None => true,
            },
        decreases state.spec_work().len() - work_index,
    {
        if let Some(worker_index) = first_feasible_worker(state, work_index) {
            let replace = match chosen {
                Some((current, _)) => candidate_precedes(state, work_index, current),
                None => true,
            };
            if replace {
                chosen = Some((work_index, worker_index));
            }
        }
        work_index += 1;
    }
    proof {
        assert(match chosen {
            Some((selected_work, selected_worker)) =>
                selected_work < state.spec_work().len()
                    && selected_worker < state.spec_workers().len()
                    && (state.spec_reservation_invariant() ==> selected_pair_admitted(
                        state,
                        selected_work as int,
                        selected_worker as int,
                    )),
            None => true,
        });
    }
    match chosen {
        Some((work_index, worker_index)) => {
            proof {
                assert(work_index < state.spec_work().len());
                assert(worker_index < state.spec_workers().len());
                if state.spec_reservation_invariant() {
                    assert(selected_pair_admitted(
                        state,
                        work_index as int,
                        worker_index as int,
                    ));
                }
            }
            Some(IndexedSelection { work_index, worker_index })
        }
        None => None,
    }
}

pub fn select_next_for_dispatch(
    state: &SchedulerState,
) -> (result: Option<(usize, usize)>)
    ensures
        match result {
            Some((work_index, worker_index)) =>
                work_index < state.spec_work().len()
                    && worker_index < state.spec_workers().len()
                    && (state.spec_reservation_invariant() ==> selected_pair_admitted(
                        state,
                        work_index as int,
                        worker_index as int,
                    )),
            None => true,
        },
{
    let selection = select_next_indexed(state)?;
    Some((selection.work_index, selection.worker_index))
}

/// Selects the next feasible item by aged-first, priority, enqueue ordinal, identity, then worker.
#[must_use]
pub fn select_next(state: &SchedulerState) -> (result: Option<Selection>)
    ensures
        state.spec_reservation_invariant() ==> match result {
            Some(selection) => selection_is_feasible(state, selection),
            None => true,
        },
{
    let selection = select_next_indexed(state)?;
            let work_records = state.work();
            let workers = state.workers();
            let selected = Selection::new(
                work_records[selection.work_index].spec().id(),
                workers[selection.worker_index].descriptor().id(),
            );
            proof {
                if state.spec_reservation_invariant() {
                    assert(selected_pair_admitted(
                        state,
                        selection.work_index as int,
                        selection.worker_index as int,
                    ));
                    reveal(selected_pair_admitted);
                    assert(work_records@ == state.spec_work());
                    assert(workers@ == state.spec_workers());
                    assert(state.spec_work()[selection.work_index as int]
                        .spec_definition().spec_id() == selected.spec_work_id());
                    assert(state.spec_workers()[selection.worker_index as int]
                        .spec_descriptor().spec_id() == selected.spec_worker_id());
                    assert(selection_is_feasible(state, selected));
                }
            }
    Some(selected)
}

/// Returns whether an item is feasible under current global and at least one worker capacity.
#[must_use]
pub fn is_feasible(state: &SchedulerState, work_id: WorkId) -> (result: bool) {
    let mut index = 0;
    while index < state.work().len()
        invariant index <= state.spec_work().len(),
        decreases state.spec_work().len() - index,
    {
        if state.work()[index].spec().id().same(&work_id) {
            return first_feasible_worker(state, index).is_some();
        }
        index += 1;
    }
    false
}

} // verus!
