//! Exact minimum-work scan for deterministic dispatch selection.

use vstd::prelude::*;

use crate::SchedulerState;

use super::super::model::IndexedSelection;
#[cfg(verus_only)]
use super::super::model::{
    chosen_pair_matches, chosen_prefix_matches, exact_selection_ready, first_candidate_worker,
    no_candidate_before, no_dispatch_candidate, selected_pair_admitted, work_has_candidate,
    work_precedes,
};
#[cfg(verus_only)]
use super::super::ordering;

verus! {

fn candidate_precedes(
    state: &SchedulerState,
    left_index: usize,
    right_index: usize,
) -> (result: bool)
    requires
        left_index < state.spec_work().len(),
        right_index < state.spec_work().len(),
    ensures result == work_precedes(state, left_index as int, right_index as int),
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

fn consider_work(
    state: &SchedulerState,
    work_index: usize,
    chosen: Option<(usize, usize)>,
    worker_index: Option<usize>,
) -> (result: Option<(usize, usize)>)
    requires
        work_index < state.spec_work().len(),
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
        match worker_index {
            Some(worker) =>
                worker < state.spec_workers().len()
                    && (state.spec_reservation_invariant() ==> selected_pair_admitted(
                        state,
                        work_index as int,
                        worker as int,
                    )),
            None => true,
        },
        exact_selection_ready(state) ==> match chosen {
            Some((selected_work, selected_worker)) => chosen_prefix_matches(
                state,
                work_index as int,
                selected_work as int,
                selected_worker as int,
            ),
            None => no_candidate_before(state, work_index as int),
        },
        exact_selection_ready(state) ==> match worker_index {
            Some(worker) => first_candidate_worker(
                state, work_index as int, worker as int,
            ),
            None => !work_has_candidate(state, work_index as int),
        },
    ensures
        match result {
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
        exact_selection_ready(state) ==> match result {
            Some((selected_work, selected_worker)) => chosen_prefix_matches(
                state,
                work_index as int + 1,
                selected_work as int,
                selected_worker as int,
            ),
            None => no_candidate_before(state, work_index as int + 1),
        },
{
    let Some(worker) = worker_index else {
        proof {
            if exact_selection_ready(state) {
                match chosen {
                    Some(_) => reveal(chosen_prefix_matches),
                    None => ordering::no_candidate_prefix_extends(
                        state, work_index as int,
                    ),
                }
            }
        }
        return chosen;
    };
    let Some((current, current_worker)) = chosen else {
        proof {
            if exact_selection_ready(state) {
                ordering::first_candidate_starts_prefix(
                    state, work_index as int, worker as int,
                );
            }
        }
        return Some((work_index, worker));
    };
    let replace = candidate_precedes(state, work_index, current);
    proof {
        if exact_selection_ready(state) && replace {
            ordering::better_candidate_replaces_prefix(
                state,
                work_index as int,
                current as int,
                current_worker as int,
                worker as int,
            );
        }
        if exact_selection_ready(state) && !replace {
            reveal(first_candidate_worker);
            assert(work_has_candidate(state, work_index as int));
            ordering::chosen_prefix_keeps_current(
                state,
                work_index as int,
                current as int,
                current_worker as int,
            );
        }
    }
    Some(if replace {
        (work_index, worker)
    } else {
        (current, current_worker)
    })
}

/// Selects the exact feasible pair by canonical work rank and first worker order.
pub(in crate::selection) fn select_next_indexed(
    state: &SchedulerState,
) -> (result: Option<IndexedSelection>)
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
        exact_selection_ready(state) ==> match result {
            Some(selection) => chosen_pair_matches(
                state,
                selection.work_index as int,
                selection.worker_index as int,
            ),
            None => no_dispatch_candidate(state),
        },
{
    let mut chosen: Option<(usize, usize)> = None;
    let mut work_index = 0;
    proof { reveal(no_candidate_before); }
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
            exact_selection_ready(state) ==> match chosen {
                Some((selected_work, selected_worker)) => chosen_prefix_matches(
                    state,
                    work_index as int,
                    selected_work as int,
                    selected_worker as int,
                ),
                None => no_candidate_before(state, work_index as int),
            },
        decreases state.spec_work().len() - work_index,
    {
        let worker = super::first_feasible_worker(state, work_index);
        proof {
            if exact_selection_ready(state) && worker.is_none() {
                reveal(work_has_candidate);
            }
        }
        chosen = consider_work(state, work_index, chosen, worker);
        work_index += 1;
    }
    proof {
        if exact_selection_ready(state) {
            match chosen {
                Some(_) => {
                    reveal(chosen_pair_matches);
                    reveal(chosen_prefix_matches);
                },
                None => {
                    reveal(no_dispatch_candidate);
                    reveal(no_candidate_before);
                },
            }
        }
    }
    let (work_index, worker_index) = chosen?;
    Some(IndexedSelection { work_index, worker_index })
}

} // verus!
