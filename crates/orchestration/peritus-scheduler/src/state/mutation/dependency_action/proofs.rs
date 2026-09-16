//! Proof helpers for exact dependency-action selection.

use vstd::prelude::*;

use crate::{SchedulerState, WorkId, WorkPhase, WorkRecord};

use super::{
    DependencyAction, action_matches, action_shape, all_succeeded, dependency_failed,
    dependency_succeeded, first_failed, no_failed, terminal_failed, terminal_succeeded,
};

verus! {

/// An actionable result cannot also satisfy the no-change branch for the same record.
pub proof fn actionable_excludes_no_change(
    state: &SchedulerState,
    record: &WorkRecord,
    action: DependencyAction,
)
    requires
        action != DependencyAction::NoChange,
        action_matches(state, record, action),
    ensures !action_matches(state, record, DependencyAction::NoChange),
{
    reveal(action_matches);
    match record.spec_phase() {
        WorkPhase::WaitingDependencies => match action {
            DependencyAction::Failed { dependency } => {
                reveal(first_failed);
                let failed_at = choose |index: int| {
                    &&& 0 <= index < record.spec_definition().spec_dependencies().len()
                    &&& record.spec_definition().spec_dependencies()[index] == dependency
                    &&& dependency_failed(state.spec_work(), dependency)
                    &&& forall |prior: int| 0 <= prior < index ==>
                        !dependency_failed(
                            state.spec_work(),
                            record.spec_definition().spec_dependencies()[prior],
                        )
                };
                reveal(no_failed);
                assert(!no_failed(
                    state.spec_work(), record.spec_definition().spec_dependencies(),
                ));
            },
            DependencyAction::Ready => {},
            DependencyAction::NoChange => assert(false),
        },
        WorkPhase::Queued => match action {
            DependencyAction::Failed { dependency } => {
                reveal(first_failed);
                let failed_at = choose |index: int| {
                    &&& 0 <= index < record.spec_definition().spec_dependencies().len()
                    &&& record.spec_definition().spec_dependencies()[index] == dependency
                    &&& dependency_failed(state.spec_work(), dependency)
                    &&& forall |prior: int| 0 <= prior < index ==>
                        !dependency_failed(
                            state.spec_work(),
                            record.spec_definition().spec_dependencies()[prior],
                        )
                };
                reveal(no_failed);
                assert(!no_failed(
                    state.spec_work(), record.spec_definition().spec_dependencies(),
                ));
            },
            DependencyAction::Ready | DependencyAction::NoChange => assert(false),
        },
        _ => assert(false),
    }
}

proof fn ordered_matching_record_is_unique(
    work: Seq<WorkRecord>,
    left: int,
    right: int,
    dependency: WorkId,
)
    requires
        SchedulerState::work_records_ordered(work),
        0 <= left < work.len(),
        0 <= right < work.len(),
        work[left].spec_definition().spec_id() == dependency,
        work[right].spec_definition().spec_id() == dependency,
    ensures left == right,
{
    if left < right {
        assert(dependency.spec_precedes(&dependency));
        WorkId::order_irreflexive(&dependency);
    } else if right < left {
        assert(dependency.spec_precedes(&dependency));
        WorkId::order_irreflexive(&dependency);
    }
}

pub(super) proof fn found_dependency_is_exact(
    state: &SchedulerState,
    dependency: WorkId,
    record: WorkRecord,
)
    requires
        state.spec_work_ordered(),
        exists |index: int| #![trigger state.spec_work()[index]]
            0 <= index < state.spec_work().len()
                && state.spec_work()[index].spec_definition().spec_id() == dependency
                && state.spec_work()[index] == record,
    ensures
        dependency_succeeded(state.spec_work(), dependency) == terminal_succeeded(record),
        dependency_failed(state.spec_work(), dependency) == terminal_failed(record),
{
    let found = choose |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id() == dependency
            && state.spec_work()[index] == record;
    if dependency_succeeded(state.spec_work(), dependency) {
        let other = choose |index: int| #![trigger state.spec_work()[index]]
            0 <= index < state.spec_work().len()
                && state.spec_work()[index].spec_definition().spec_id() == dependency
                && terminal_succeeded(state.spec_work()[index]);
        ordered_matching_record_is_unique(state.spec_work(), found, other, dependency);
    }
    if dependency_failed(state.spec_work(), dependency) {
        let other = choose |index: int| #![trigger state.spec_work()[index]]
            0 <= index < state.spec_work().len()
                && state.spec_work()[index].spec_definition().spec_id() == dependency
                && terminal_failed(state.spec_work()[index]);
        ordered_matching_record_is_unique(state.spec_work(), found, other, dependency);
    }
    if terminal_succeeded(record) {
        assert(dependency_succeeded(state.spec_work(), dependency)) by {
            assert(exists |index: int| index == found
                && 0 <= index < state.spec_work().len()
                && state.spec_work()[index].spec_definition().spec_id() == dependency
                && terminal_succeeded(state.spec_work()[index]));
        }
    }
    if terminal_failed(record) {
        assert(dependency_failed(state.spec_work(), dependency)) by {
            assert(exists |index: int| index == found
                && 0 <= index < state.spec_work().len()
                && state.spec_work()[index].spec_definition().spec_id() == dependency
                && terminal_failed(state.spec_work()[index]));
        }
    }
}

pub(super) proof fn establish_final_action(
    state: &SchedulerState,
    record: &WorkRecord,
    dependencies: Seq<WorkId>,
    all_success: bool,
    waiting: bool,
    action: DependencyAction,
)
    requires
        dependencies == record.spec_definition().spec_dependencies(),
        record.spec_phase() == WorkPhase::WaitingDependencies
            || record.spec_phase() == WorkPhase::Queued,
        waiting == (record.spec_phase() == WorkPhase::WaitingDependencies),
        state.spec_work_ordered() ==>
            no_failed(state.spec_work(), dependencies),
        state.spec_work_ordered() ==>
            all_success == all_succeeded(state.spec_work(), dependencies),
        action == if all_success && waiting {
            DependencyAction::Ready
        } else {
            DependencyAction::NoChange
        },
    ensures
        action_shape(record, action),
        state.spec_work_ordered() ==> action_matches(state, record, action),
{
    reveal(action_shape);
    reveal(action_matches);
    match action {
        DependencyAction::Ready => {
            assert(all_success);
            assert(waiting);
            assert(record.spec_phase() == WorkPhase::WaitingDependencies);
        },
        DependencyAction::NoChange => {
            if record.spec_phase() == WorkPhase::WaitingDependencies {
                assert(waiting);
                assert(!all_success);
                if state.spec_work_ordered() {
                    assert(!all_succeeded(state.spec_work(), dependencies));
                }
            } else {
                assert(record.spec_phase() == WorkPhase::Queued);
            }
        },
        DependencyAction::Failed { .. } => assert(false),
    }
}

} // verus!
