//! Pure deterministic bounded-bypass selection.

use crate::{SchedulerState, WorkId};

mod capacity;
mod model;

mod scan;

pub use capacity::worker_reservation_count;
pub use model::Selection;

use model::IndexedSelection;

pub fn select_next_for_dispatch(state: &SchedulerState) -> Option<(usize, usize)> {
    let selection = scan::select_next_indexed(state)?;
    Some((selection.work_index, selection.worker_index))
}
#[doc = " Selects the next feasible item by aged-first, priority, enqueue ordinal, identity, then worker."]
#[must_use]
pub fn select_next(state: &SchedulerState) -> Option<Selection> {
    let selection: IndexedSelection = scan::select_next_indexed(state)?;
    let work_records = state.work();
    let workers = state.workers();
    let selected = Selection::new(
        work_records[selection.work_index].spec().id(),
        workers[selection.worker_index].descriptor().id(),
    );
    Some(selected)
}
#[doc = " Returns whether an item is feasible under current global and at least one worker capacity."]
#[must_use]
pub fn is_feasible(state: &SchedulerState, work_id: WorkId) -> bool {
    let mut index = 0;
    while index < state.work().len() {
        if state.work()[index].spec().id().same(&work_id) {
            return scan::first_feasible_worker(state, index).is_some();
        }
        index += 1;
    }
    false
}
