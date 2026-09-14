//! Natural termination measure for dependency refresh mutations.

use vstd::prelude::*;

use crate::state::mutation;
use crate::{SchedulerState, WorkId, WorkPhase, WorkRecord, WorkTerminal};

use super::dependency_action::{DependencyAction, action_matches};
use super::dependency_round::{
    DependencyChange, change_applicable, change_target_matches, dependency_fixed_point,
    dependency_step_matches,
};

verus! {

/// Remaining dependency-propagation work carried by one retained record.
pub open spec fn dependency_weight(record: WorkRecord) -> nat {
    match record.spec_phase() {
        WorkPhase::WaitingDependencies => 2,
        WorkPhase::Queued => 1,
        _ => 0,
    }
}

/// Finite natural measure for all dependency-propagation work.
pub open spec fn dependency_measure(work: Seq<WorkRecord>) -> nat
    decreases work.len(),
{
    if work.len() == 0 {
        0
    } else {
        dependency_weight(work.first()) + dependency_measure(work.drop_first())
    }
}

pub(super) proof fn dependency_measure_is_bounded(work: Seq<WorkRecord>)
    ensures dependency_measure(work) <= 2 * work.len(),
    decreases work.len(),
{
    reveal(dependency_measure);
    if work.len() > 0 {
        dependency_measure_is_bounded(work.drop_first());
        reveal(dependency_weight);
    }
}

proof fn record_weight_is_bounded_by_measure(work: Seq<WorkRecord>, index: int)
    requires 0 <= index < work.len(),
    ensures dependency_weight(work[index]) <= dependency_measure(work),
    decreases work.len(),
{
    reveal(dependency_measure);
    if index > 0 {
        record_weight_is_bounded_by_measure(work.drop_first(), index - 1);
        assert(work.drop_first()[index - 1] == work[index]);
    }
}

/// A zero dependency measure cannot retain a dependency-actionable phase.
pub(super) proof fn zero_measure_is_fixed_point(state: &SchedulerState)
    requires dependency_measure(state.spec_work()) == 0,
    ensures dependency_fixed_point(state),
{
    reveal(dependency_fixed_point);
    assert forall |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len() implies
            action_matches(
                state,
                &state.spec_work()[index],
                DependencyAction::NoChange,
            ) by {
        record_weight_is_bounded_by_measure(state.spec_work(), index);
        reveal(dependency_weight);
        reveal(action_matches);
    }
}

pub(super) proof fn propagation_is_complete(
    state: &SchedulerState,
    remaining: nat,
    second_half: bool,
)
    requires
        dependency_fixed_point(state)
            || (remaining == 0
                && dependency_measure(state.spec_work())
                    < super::dependency_budget::dependency_budget(remaining, second_half)),
    ensures dependency_fixed_point(state),
{
    if !dependency_fixed_point(state) {
        reveal(super::dependency_budget::dependency_budget);
        assert(dependency_measure(state.spec_work()) == 0);
        zero_measure_is_fixed_point(state);
    }
}

proof fn matching_work_indices_are_equal(
    work: Seq<WorkRecord>,
    left: int,
    right: int,
    id: WorkId,
)
    requires
        crate::verified::work_identities_unique(work),
        0 <= left < work.len(),
        0 <= right < work.len(),
        work[left].spec_definition().spec_id() == id,
        work[right].spec_definition().spec_id() == id,
    ensures left == right,
{
    reveal(crate::verified::work_identities_unique);
}

proof fn dependency_measure_replacement(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    changed: int,
)
    requires
        before.len() == after.len(),
        0 <= changed < before.len(),
        forall |other: int| #![auto]
            0 <= other < before.len() && other != changed ==> after[other] == before[other],
    ensures
        dependency_measure(after) + dependency_weight(before[changed])
            == dependency_measure(before) + dependency_weight(after[changed]),
    decreases before.len(),
{
    reveal(dependency_measure);
    if changed == 0 {
        assert(before.drop_first() =~= after.drop_first()) by {
            assert forall |index: int| 0 <= index < before.drop_first().len()
                implies before.drop_first()[index] == after.drop_first()[index] by {
                assert(0 < index + 1 < before.len());
                assert(after[index + 1] == before[index + 1]);
            }
        }
    } else {
        assert(before.first() == after.first());
        assert(before.drop_first().len() == after.drop_first().len());
        assert forall |other: int| #![auto]
            0 <= other < before.drop_first().len() && other != changed - 1
            implies after.drop_first()[other] == before.drop_first()[other] by {
            assert(other + 1 != changed);
            assert(after[other + 1] == before[other + 1]);
        }
        dependency_measure_replacement(
            before.drop_first(), after.drop_first(), changed - 1,
        );
        assert(before.drop_first()[changed - 1] == before[changed]);
        assert(after.drop_first()[changed - 1] == after[changed]);
    }
}

proof fn ready_update_decreases_measure(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    source: int,
)
    requires
        crate::verified::work_identities_unique(before),
        0 <= source < before.len(),
        before[source].spec_definition().spec_id() == id,
        before[source].spec_phase() == WorkPhase::WaitingDependencies,
        mutation::work_phase_update_matches(before, after, id, WorkPhase::Queued, true),
    ensures dependency_measure(after) + 1 == dependency_measure(before),
{
    reveal(mutation::work_phase_update_matches);
    let changed = choose |index: int| #![trigger before[index]] {
        &&& 0 <= index < before.len()
        &&& mutation::work_record_update_matches(
            before[index], after[index], id, WorkPhase::Queued,
        )
        &&& after[index].spec_retry_cause() == before[index].spec_retry_cause()
        &&& after[index].spec_terminal() == before[index].spec_terminal()
        &&& forall |other: int| #![auto]
            0 <= other < before.len() && other != index ==> after[other] == before[other]
    };
    reveal(mutation::work_record_update_matches);
    matching_work_indices_are_equal(before, source, changed, id);
    dependency_measure_replacement(before, after, changed);
    reveal(dependency_weight);
}

proof fn failed_update_decreases_measure(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    source: int,
    dependency: WorkId,
)
    requires
        crate::verified::work_identities_unique(before),
        0 <= source < before.len(),
        before[source].spec_definition().spec_id() == id,
        before[source].spec_phase() == WorkPhase::WaitingDependencies
            || before[source].spec_phase() == WorkPhase::Queued,
        mutation::work_terminal_update_matches(
            before,
            after,
            id,
            WorkTerminal::DependencyFailed { dependency },
            true,
        ),
    ensures dependency_measure(after) < dependency_measure(before),
{
    reveal(mutation::work_terminal_update_matches);
    let changed = choose |index: int| #![trigger before[index]] {
        &&& 0 <= index < before.len()
        &&& mutation::work_record_update_matches(
            before[index], after[index], id, WorkPhase::Terminal,
        )
        &&& after[index].spec_retry_cause().is_none()
        &&& after[index].spec_terminal() == Some(WorkTerminal::DependencyFailed { dependency })
        &&& forall |other: int| #![auto]
            0 <= other < before.len() && other != index ==> after[other] == before[other]
    };
    reveal(mutation::work_record_update_matches);
    matching_work_indices_are_equal(before, source, changed, id);
    dependency_measure_replacement(before, after, changed);
    reveal(dependency_weight);
}

/// Every actual actionable dependency update strictly reduces the natural measure.
pub(super) proof fn dependency_step_decreases_measure(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
)
    requires
        before.spec_reservation_reducer_ready(),
        dependency_step_matches(before, after, change),
    ensures dependency_measure(after.spec_work()) < dependency_measure(before.spec_work()),
{
    reveal(dependency_step_matches);
    reveal(change_applicable);
    let source = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& change_target_matches(before.spec_work()[index], change)
    };
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservation_invariant_parts);
    reveal(change_target_matches);
    match change.spec_action() {
        DependencyAction::Ready => ready_update_decreases_measure(
            before.spec_work(),
            after.spec_work(),
            change.spec_work_id(),
            source,
        ),
        DependencyAction::Failed { dependency } => failed_update_decreases_measure(
            before.spec_work(),
            after.spec_work(),
            change.spec_work_id(),
            source,
            dependency,
        ),
        DependencyAction::NoChange => assert(false),
    }
}

} // verus!
