//! Finite application of one immutable dependency-refresh change set.

use crate::state::mutation;
use crate::{SchedulerState, WorkPhase, WorkTerminal};

use super::{DependencyAction, DependencyChange};

#[doc = " Applies one already-selected dependency change without adding caller preconditions."]
fn apply_one(state: &mut SchedulerState, change: DependencyChange) -> bool {
    match change.action() {
        DependencyAction::Ready => {
            mutation::set_work_phase(state, change.work_id(), WorkPhase::Queued)
        }
        DependencyAction::Failed { dependency } => mutation::terminalize_work(
            state,
            change.work_id(),
            WorkTerminal::DependencyFailed { dependency },
        ),
        DependencyAction::NoChange => false,
    }
}
#[doc = " Applies an exact immutable-snapshot change set in its collected order."]
pub(super) fn apply_dependency_changes(state: &mut SchedulerState, changes: &[DependencyChange]) {
    let mut index = 0;
    while index < changes.len() {
        let change = changes[index];
        let _applied = apply_one(state, change);
        index += 1;
    }
}
