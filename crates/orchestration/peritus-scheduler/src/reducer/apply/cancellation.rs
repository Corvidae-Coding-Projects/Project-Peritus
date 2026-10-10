//! Exact finite parent closure for production cancellation commands.

use crate::{SchedulerState, WorkId, WorkPhase, WorkRecord};

pub(super) mod command;
mod ids;

mod update;

fn closure(work: &[WorkRecord], root: WorkId) -> Vec<WorkId> {
    let mut reached = vec![root];
    loop {
        let mut changed = false;
        let mut index = 0;
        while index < work.len() {
            let record = &work[index];
            match record.spec().parent() {
                Some(parent) if ids::contains(&reached, parent) => {
                    let id = record.spec().id();
                    if ids::insert(&mut reached, id) {
                        changed = true;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        if !changed {
            return reached;
        }
    }
}
#[doc = " Selects the exact pre-state cancellation targets in retained canonical order."]
pub(super) fn affected(work: &[WorkRecord], root: WorkId, descendants: bool) -> Vec<WorkId> {
    let reached = if descendants { closure(work, root) } else { vec![root] };
    let mut result = Vec::new();
    let mut index = 0;
    while index < work.len() {
        let record = &work[index];
        if !record.phase().same(WorkPhase::Terminal) && ids::contains(&reached, record.spec().id())
        {
            result.push(record.spec().id());
        }
        index += 1;
    }
    result
}

#[doc = " Applies the exact selected cancellation sequence while retaining every reservation."]
pub(super) fn cancel_retained(
    state: &mut SchedulerState,
    root: WorkId,
    descendants: bool,
) -> (Vec<WorkId>, bool) {
    let targets = affected(state.work(), root, descendants);
    let complete = update::apply_updates(state, &targets);
    (targets, complete)
}
