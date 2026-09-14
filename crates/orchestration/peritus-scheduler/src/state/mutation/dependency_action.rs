//! Exact dependency action selected by the production refresh scan.

use vstd::prelude::*;

use crate::{SchedulerState, WorkId, WorkPhase, WorkRecord, WorkTerminal};

#[cfg(verus_only)]
mod proofs;

#[cfg(verus_only)]
pub(super) use proofs::actionable_excludes_no_change;
#[cfg(verus_only)]
use proofs::{establish_final_action, found_dependency_is_exact};

verus! {

/// One deterministic dependency-driven work change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyAction {
    /// The current phase and observed dependencies require no change.
    NoChange,
    /// Every dependency succeeded, so waiting work becomes queued.
    Ready,
    /// The first terminal non-success dependency determines failure.
    Failed {
        /// First failed dependency in the work specification's canonical order.
        dependency: WorkId,
    },
}

pub open spec fn terminal_succeeded(record: WorkRecord) -> bool {
    matches!(record.spec_terminal(), Some(WorkTerminal::Succeeded { .. }))
}

pub open spec fn terminal_failed(record: WorkRecord) -> bool {
    match record.spec_terminal() {
        Some(WorkTerminal::Succeeded { .. }) | None => false,
        Some(_) => true,
    }
}

pub open spec fn dependency_succeeded(work: Seq<WorkRecord>, dependency: WorkId) -> bool {
    exists |index: int| #![trigger work[index]]
        0 <= index < work.len()
            && work[index].spec_definition().spec_id() == dependency
            && terminal_succeeded(work[index])
}

pub open spec fn dependency_failed(work: Seq<WorkRecord>, dependency: WorkId) -> bool {
    exists |index: int| #![trigger work[index]]
        0 <= index < work.len()
            && work[index].spec_definition().spec_id() == dependency
            && terminal_failed(work[index])
}

pub open spec fn all_succeeded(work: Seq<WorkRecord>, dependencies: Seq<WorkId>) -> bool {
    forall |index: int| 0 <= index < dependencies.len() ==>
        dependency_succeeded(work, dependencies[index])
}

pub open spec fn no_failed(work: Seq<WorkRecord>, dependencies: Seq<WorkId>) -> bool {
    forall |index: int| 0 <= index < dependencies.len() ==>
        !dependency_failed(work, dependencies[index])
}

pub open spec fn first_failed(
    work: Seq<WorkRecord>,
    dependencies: Seq<WorkId>,
    dependency: WorkId,
) -> bool {
    exists |index: int| {
        &&& 0 <= index < dependencies.len()
        &&& dependencies[index] == dependency
        &&& dependency_failed(work, dependency)
        &&& forall |prior: int| 0 <= prior < index ==>
            !dependency_failed(work, dependencies[prior])
    }
}

/// Exact action on a canonically ordered state.
pub open spec fn action_matches(
    state: &SchedulerState,
    record: &WorkRecord,
    action: DependencyAction,
) -> bool {
    let work = state.spec_work();
    let dependencies = record.spec_definition().spec_dependencies();
    match record.spec_phase() {
        WorkPhase::WaitingDependencies => match action {
            DependencyAction::Failed { dependency } =>
                first_failed(work, dependencies, dependency),
            DependencyAction::Ready =>
                no_failed(work, dependencies) && all_succeeded(work, dependencies),
            DependencyAction::NoChange =>
                no_failed(work, dependencies) && !all_succeeded(work, dependencies),
        },
        WorkPhase::Queued => match action {
            DependencyAction::Failed { dependency } =>
                first_failed(work, dependencies, dependency),
            DependencyAction::NoChange => no_failed(work, dependencies),
            DependencyAction::Ready => false,
        },
        _ => matches!(action, DependencyAction::NoChange),
    }
}

/// Result shape that is sound even before ordered-state validation.
pub open spec fn action_shape(record: &WorkRecord, action: DependencyAction) -> bool {
    match action {
        DependencyAction::NoChange => true,
        DependencyAction::Ready => record.spec_phase() == WorkPhase::WaitingDependencies,
        DependencyAction::Failed { dependency } =>
            record.spec_definition().spec_dependencies().contains(dependency),
    }
}


const fn finish_dependency_action(
    _state: &SchedulerState,
    record: &WorkRecord,
    all_success: bool,
) -> (action: DependencyAction)
    requires
        record.spec_phase() == WorkPhase::WaitingDependencies
            || record.spec_phase() == WorkPhase::Queued,
        _state.spec_work_ordered() ==>
            no_failed(
                _state.spec_work(),
                record.spec_definition().spec_dependencies(),
            ),
        _state.spec_work_ordered() ==>
            all_success == all_succeeded(
                _state.spec_work(),
                record.spec_definition().spec_dependencies(),
            ),
    ensures
        action_shape(record, action),
        _state.spec_work_ordered() ==> action_matches(_state, record, action),
{
    let waiting = record.phase().same(WorkPhase::WaitingDependencies);
    let action = if all_success && waiting {
        DependencyAction::Ready
    } else {
        DependencyAction::NoChange
    };
    proof {
        establish_final_action(
            _state,
            record,
            record.spec_definition().spec_dependencies(),
            all_success,
            waiting,
            action,
        );
    }
    action
}

/// Selects the exact action used by dependency propagation.
pub(super) fn dependency_action(
    state: &SchedulerState,
    record: &WorkRecord,
) -> (action: DependencyAction)
    ensures
        action_shape(record, action),
        state.spec_work_ordered() ==> action_matches(state, record, action),
{
    if !matches!(record.phase(), WorkPhase::WaitingDependencies | WorkPhase::Queued) {
        proof {
            reveal(action_shape);
            reveal(action_matches);
        }
        return DependencyAction::NoChange;
    }
    let dependencies = record.spec().dependencies();
    let ghost spec_dependencies = record.spec_definition().spec_dependencies();
    let ghost initial_phase = record.spec_phase();
    proof {
        assert(dependencies@ == spec_dependencies);
        assert(initial_phase == WorkPhase::WaitingDependencies
            || initial_phase == WorkPhase::Queued);
    }
    let mut all_success = true;
    let mut index = 0;
    while index < dependencies.len()
        invariant
            index <= dependencies@.len(),
            dependencies@ == spec_dependencies,
            spec_dependencies == record.spec_definition().spec_dependencies(),
            record.spec_phase() == initial_phase,
            initial_phase == WorkPhase::WaitingDependencies
                || initial_phase == WorkPhase::Queued,
            state.spec_work_ordered() ==> forall |prior: int| 0 <= prior < index ==>
                !dependency_failed(state.spec_work(), spec_dependencies[prior]),
            state.spec_work_ordered() ==> (all_success == (forall |prior: int|
                0 <= prior < index ==>
                    dependency_succeeded(state.spec_work(), spec_dependencies[prior]))),
        decreases dependencies@.len() - index,
    {
        let dependency = dependencies[index];
        match state.work_item(dependency) {
            Some(observed) => {
                proof {
                    if state.spec_work_ordered() {
                        found_dependency_is_exact(state, dependency, *observed);
                    }
                }
                match observed.terminal() {
                    Some(WorkTerminal::Succeeded { .. }) => {},
                    Some(_) => {
                        proof {
                            assert(spec_dependencies[index as int] == dependency);
                            assert(spec_dependencies.contains(dependency));
                            if state.spec_work_ordered() {
                                assert(first_failed(
                                    state.spec_work(), spec_dependencies, dependency,
                                )) by {
                                    assert(exists |failed_at: int| failed_at == index
                                        && 0 <= failed_at < spec_dependencies.len()
                                        && spec_dependencies[failed_at] == dependency
                                        && dependency_failed(state.spec_work(), dependency)
                                        && forall |prior: int| 0 <= prior < failed_at ==>
                                            !dependency_failed(
                                                state.spec_work(), spec_dependencies[prior],
                                            ));
                                }
                            }
                            reveal(action_shape);
                            reveal(action_matches);
                        }
                        return DependencyAction::Failed { dependency };
                    },
                    None => all_success = false,
                }
            },
            None => all_success = false,
        }
        index += 1;
    }
    proof {
        if state.spec_work_ordered() {
            assert(no_failed(state.spec_work(), spec_dependencies));
            assert(all_success == all_succeeded(state.spec_work(), spec_dependencies));
        }
    }
    finish_dependency_action(state, record, all_success)
}

} // verus!
