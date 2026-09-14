//! Proofs for the exact bounded-bypass ordering used by selection.

use vstd::prelude::*;

use crate::{SchedulerState, WorkId};

use super::model::{
    chosen_prefix_matches, first_candidate_worker, no_candidate_before, work_has_candidate,
    work_precedes,
};

verus! {

pub(super) proof fn work_precedes_irreflexive(state: &SchedulerState, index: int)
    requires 0 <= index < state.spec_work().len(),
    ensures !work_precedes(state, index, index),
{
    reveal(work_precedes);
    WorkId::order_irreflexive(
        &state.spec_work()[index].spec_definition().spec_id(),
    );
}

pub(super) proof fn work_precedes_transitive(
    state: &SchedulerState,
    left: int,
    middle: int,
    right: int,
)
    requires
        0 <= left < state.spec_work().len(),
        0 <= middle < state.spec_work().len(),
        0 <= right < state.spec_work().len(),
        work_precedes(state, left, middle),
        work_precedes(state, middle, right),
    ensures work_precedes(state, left, right),
{
    let left_id = state.spec_work()[left].spec_definition().spec_id();
    let middle_id = state.spec_work()[middle].spec_definition().spec_id();
    let right_id = state.spec_work()[right].spec_definition().spec_id();
    reveal(work_precedes);
    if (state.spec_work()[left].spec_bypasses()
            >= state.spec_binding().spec_limits().spec_bypass_count())
        == (state.spec_work()[right].spec_bypasses()
            >= state.spec_binding().spec_limits().spec_bypass_count())
        && state.spec_work()[left].spec_definition().spec_priority()
            == state.spec_work()[right].spec_definition().spec_priority()
        && state.spec_work()[left].spec_enqueue_ordinal()
            == state.spec_work()[right].spec_enqueue_ordinal()
    {
        WorkId::order_transitive(&left_id, &middle_id, &right_id);
    }
}

pub(super) proof fn no_candidate_prefix_extends(
    state: &SchedulerState,
    end: int,
)
    requires
        0 <= end < state.spec_work().len(),
        no_candidate_before(state, end),
        !work_has_candidate(state, end),
    ensures no_candidate_before(state, end + 1),
{
    reveal(no_candidate_before);
}

pub(super) proof fn first_candidate_starts_prefix(
    state: &SchedulerState,
    end: int,
    worker_index: int,
)
    requires
        0 <= end < state.spec_work().len(),
        no_candidate_before(state, end),
        first_candidate_worker(state, end, worker_index),
    ensures chosen_prefix_matches(state, end + 1, end, worker_index),
{
    reveal(first_candidate_worker);
    reveal(no_candidate_before);
    reveal(chosen_prefix_matches);
    work_precedes_irreflexive(state, end);
}

pub(super) proof fn chosen_prefix_keeps_current(
    state: &SchedulerState,
    end: int,
    selected_work: int,
    selected_worker: int,
)
    requires
        0 <= end < state.spec_work().len(),
        chosen_prefix_matches(state, end, selected_work, selected_worker),
        work_has_candidate(state, end),
        !work_precedes(state, end, selected_work),
    ensures chosen_prefix_matches(state, end + 1, selected_work, selected_worker),
{
    reveal(chosen_prefix_matches);
}

pub(super) proof fn better_candidate_replaces_prefix(
    state: &SchedulerState,
    end: int,
    selected_work: int,
    selected_worker: int,
    new_worker: int,
)
    requires
        0 <= end < state.spec_work().len(),
        chosen_prefix_matches(state, end, selected_work, selected_worker),
        first_candidate_worker(state, end, new_worker),
        work_precedes(state, end, selected_work),
    ensures chosen_prefix_matches(state, end + 1, end, new_worker),
{
    reveal(chosen_prefix_matches);
    work_precedes_irreflexive(state, end);
    assert forall |other: int| #![trigger work_has_candidate(state, other)]
        0 <= other < end && work_has_candidate(state, other)
            implies !work_precedes(state, other, end) by {
        if work_precedes(state, other, end) {
            work_precedes_transitive(state, other, end, selected_work);
            assert(false);
        }
    }
}

} // verus!
