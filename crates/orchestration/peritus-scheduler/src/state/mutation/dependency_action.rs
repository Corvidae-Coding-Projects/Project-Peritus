//! Exact dependency action selected by the production refresh scan.

use crate::{SchedulerState, WorkId, WorkPhase, WorkRecord, WorkTerminal};

#[doc = " One deterministic dependency-driven work change."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyAction {
    #[doc = " The current phase and observed dependencies require no change."]
    NoChange,
    #[doc = " Every dependency succeeded, so waiting work becomes queued."]
    Ready,
    #[doc = " The first terminal non-success dependency determines failure."]
    Failed {
        #[doc = " First failed dependency in the work specification\'s canonical order."]
        dependency: WorkId,
    },
}
const fn finish_dependency_action(
    _state: &SchedulerState,
    record: &WorkRecord,
    all_success: bool,
) -> DependencyAction {
    let waiting = record.phase().same(WorkPhase::WaitingDependencies);
    let action =
        if all_success && waiting { DependencyAction::Ready } else { DependencyAction::NoChange };
    action
}
#[doc = " Selects the exact action used by dependency propagation."]
pub(super) fn dependency_action(state: &SchedulerState, record: &WorkRecord) -> DependencyAction {
    if !matches!(record.phase(), WorkPhase::WaitingDependencies | WorkPhase::Queued) {
        return DependencyAction::NoChange;
    }
    let dependencies = record.spec().dependencies();
    let mut all_success = true;
    let mut index = 0;
    while index < dependencies.len() {
        let dependency = dependencies[index];
        match state.work_item(dependency) {
            Some(observed) => match observed.terminal() {
                Some(WorkTerminal::Succeeded { .. }) => {}
                Some(_) => {
                    return DependencyAction::Failed { dependency };
                }
                None => all_success = false,
            },
            None => all_success = false,
        }
        index += 1;
    }
    finish_dependency_action(state, record, all_success)
}
