//! Exact ordered collection of dependency refresh changes.

use crate::{SchedulerState, WorkId};

mod apply;

use apply::apply_dependency_changes;

use super::dependency_action::{DependencyAction, dependency_action};

#[doc = " One work mutation selected from the pre-round dependency snapshot."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DependencyChange {
    work_id: WorkId,
    action: DependencyAction,
}
impl DependencyChange {
    const fn new(work_id: WorkId, action: DependencyAction) -> Self {
        Self { work_id, action }
    }
    #[doc = " Returns the work identity selected in the immutable scan."]
    pub(super) const fn work_id(&self) -> WorkId {
        self.work_id
    }
    #[doc = " Returns the dependency action selected in the immutable scan."]
    pub(super) const fn action(&self) -> DependencyAction {
        self.action
    }
}
#[doc = " Collects the exact production refresh actions from one immutable state snapshot."]
pub(super) fn collect_dependency_changes(state: &SchedulerState) -> Vec<DependencyChange> {
    let work = state.work();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < work.len() {
        let record = &work[index];
        let action = dependency_action(state, record);
        match action {
            DependencyAction::NoChange => {}
            DependencyAction::Ready | DependencyAction::Failed { .. } => {
                let change = DependencyChange::new(record.spec().id(), action);
                changes.push(change);
            }
        }
        index += 1;
    }
    changes
}
#[doc = " Collects and applies exactly one immutable dependency-refresh round."]
#[doc = ""]
#[doc = " `changed` reports whether this round selected at least one mutation. A caller may iterate"]
#[doc = " rounds to a fixed point separately; this function makes no fixed-point claim."]
pub(super) fn apply_dependency_round(state: &mut SchedulerState) -> bool {
    let changes = collect_dependency_changes(state);
    apply_dependency_changes(state, &changes);
    !changes.is_empty()
}
