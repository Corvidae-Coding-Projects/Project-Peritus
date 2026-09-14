//! Pure deterministic bounded-bypass selection.

use vstd::prelude::*;

use crate::{SchedulerState, WorkId};

mod capacity;
mod model;
#[cfg(verus_only)]
mod ordering;
mod scan;

pub use capacity::worker_reservation_count;
pub use model::Selection;

use model::IndexedSelection;
#[cfg(verus_only)]
pub(crate) use model::{
    admitted_work_is_unreserved, chosen_pair_matches as exact_chosen_pair,
    exact_selection_ready as exact_selection_available, no_dispatch_candidate,
    selected_pair_admitted, selected_pair_feasible, selected_pair_feasible_parts,
};
#[cfg(verus_only)]
use model::{
    chosen_pair_matches, exact_selection_ready, selection_is_exact, selection_is_feasible,
};

verus! {

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
        exact_selection_ready(state) ==> match result {
            Some((work_index, worker_index)) => chosen_pair_matches(
                state, work_index as int, worker_index as int,
            ),
            None => no_dispatch_candidate(state),
        },
{
    let selection = scan::select_next_indexed(state)?;
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
        exact_selection_ready(state) ==> match result {
            Some(selection) => selection_is_exact(state, selection),
            None => no_dispatch_candidate(state),
        },
{
    let selection: IndexedSelection = scan::select_next_indexed(state)?;
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
        if exact_selection_ready(state) {
            assert(chosen_pair_matches(
                state,
                selection.work_index as int,
                selection.worker_index as int,
            ));
            reveal(selection_is_exact);
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
            return scan::first_feasible_worker(state, index).is_some();
        }
        index += 1;
    }
    false
}

} // verus!
