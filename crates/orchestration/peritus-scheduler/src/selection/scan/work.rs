//! Exact minimum-work scan for deterministic dispatch selection.

use crate::SchedulerState;

use super::super::model::IndexedSelection;

fn candidate_precedes(state: &SchedulerState, left_index: usize, right_index: usize) -> bool {
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
) -> Option<(usize, usize)> {
    let Some(worker) = worker_index else {
        return chosen;
    };
    let Some((current, current_worker)) = chosen else {
        return Some((work_index, worker));
    };
    let replace = candidate_precedes(state, work_index, current);
    Some(if replace { (work_index, worker) } else { (current, current_worker) })
}
#[doc = " Selects the exact feasible pair by canonical work rank and first worker order."]
pub(in crate::selection) fn select_next_indexed(
    state: &SchedulerState,
) -> Option<IndexedSelection> {
    let mut chosen: Option<(usize, usize)> = None;
    let mut work_index = 0;
    while work_index < state.work().len() {
        let worker = super::first_feasible_worker(state, work_index);
        chosen = consider_work(state, work_index, chosen, worker);
        work_index += 1;
    }
    let (work_index, worker_index) = chosen?;
    Some(IndexedSelection { work_index, worker_index })
}
