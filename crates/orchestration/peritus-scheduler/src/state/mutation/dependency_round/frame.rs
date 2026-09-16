//! Exact work-layout and state-frame facts for one dependency refresh round.

use vstd::prelude::*;

use crate::state::mutation;
use crate::{SchedulerState, WorkId, WorkPhase, WorkRecord, WorkTerminal};

use super::{DependencyChange, dependency_step_matches, work_id_layout_matches};

verus! {
pub(super) proof fn phase_update_preserves_ids(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    phase: WorkPhase,
    found: bool,
)
    requires mutation::work_phase_update_matches(before, after, id, phase, found),
    ensures work_id_layout_matches(before, after),
{
    reveal(mutation::work_phase_update_matches);
    reveal(work_id_layout_matches);
    if found {
        let changed = choose |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& mutation::work_record_update_matches(before[index], after[index], id, phase)
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != index ==>
                    after[other] == before[other]
        };
        reveal(mutation::work_record_update_matches);
        WorkRecord::lifecycle_update_fields(&before[changed], &after[changed]);
        assert forall |index: int| #![trigger before[index]] 0 <= index < before.len()
            implies after[index].spec_definition().spec_id()
                == before[index].spec_definition().spec_id() by {
            if index != changed {
                assert(after[index] == before[index]);
            }
        }
    }
}

pub(super) proof fn terminal_update_preserves_ids(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    terminal: WorkTerminal,
    found: bool,
)
    requires mutation::work_terminal_update_matches(before, after, id, terminal, found),
    ensures work_id_layout_matches(before, after),
{
    reveal(mutation::work_terminal_update_matches);
    reveal(work_id_layout_matches);
    if found {
        let changed = choose |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& mutation::work_record_update_matches(
                before[index], after[index], id, WorkPhase::Terminal,
            )
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != index ==>
                    after[other] == before[other]
        };
        reveal(mutation::work_record_update_matches);
        WorkRecord::lifecycle_update_fields(&before[changed], &after[changed]);
        assert forall |index: int| #![trigger before[index]] 0 <= index < before.len()
            implies after[index].spec_definition().spec_id()
                == before[index].spec_definition().spec_id() by {
            if index != changed {
                assert(after[index] == before[index]);
            }
        }
    }
}

pub(super) proof fn dependency_step_preserves_ids(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
)
    requires dependency_step_matches(before, after, change),
    ensures work_id_layout_matches(before.spec_work(), after.spec_work()),
{
    reveal(dependency_step_matches);
    match change.spec_action() {
        super::DependencyAction::Ready => phase_update_preserves_ids(
            before.spec_work(),
            after.spec_work(),
            change.spec_work_id(),
            WorkPhase::Queued,
            true,
        ),
        super::DependencyAction::Failed { dependency } => terminal_update_preserves_ids(
            before.spec_work(),
            after.spec_work(),
            change.spec_work_id(),
            WorkTerminal::DependencyFailed { dependency },
            true,
        ),
        super::DependencyAction::NoChange => assert(false),
    }
}

pub(super) proof fn dependency_step_preserves_other_record(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
    index: int,
)
    requires
        dependency_step_matches(before, after, change),
        0 <= index < before.spec_work().len(),
        before.spec_work()[index].spec_definition().spec_id()
            != change.spec_work_id(),
    ensures after.spec_work()[index] == before.spec_work()[index],
{
    reveal(dependency_step_matches);
    match change.spec_action() {
        super::DependencyAction::Ready => {
            reveal(mutation::work_phase_update_matches);
            let changed = choose |at: int| #![trigger before.spec_work()[at]] {
                &&& 0 <= at < before.spec_work().len()
                &&& mutation::work_record_update_matches(
                    before.spec_work()[at], after.spec_work()[at],
                    change.spec_work_id(), WorkPhase::Queued,
                )
                &&& forall |other: int| #![auto]
                    0 <= other < before.spec_work().len() && other != at ==>
                        after.spec_work()[other] == before.spec_work()[other]
            };
            reveal(mutation::work_record_update_matches);
            assert(changed != index);
        },
        super::DependencyAction::Failed { dependency } => {
            reveal(mutation::work_terminal_update_matches);
            let changed = choose |at: int| #![trigger before.spec_work()[at]] {
                &&& 0 <= at < before.spec_work().len()
                &&& mutation::work_record_update_matches(
                    before.spec_work()[at], after.spec_work()[at],
                    change.spec_work_id(), WorkPhase::Terminal,
                )
                &&& after.spec_work()[at].spec_terminal()
                    == Some(WorkTerminal::DependencyFailed { dependency })
                &&& forall |other: int| #![auto]
                    0 <= other < before.spec_work().len() && other != at ==>
                        after.spec_work()[other] == before.spec_work()[other]
            };
            reveal(mutation::work_record_update_matches);
            assert(changed != index);
        },
        super::DependencyAction::NoChange => assert(false),
    }
}

pub(super) proof fn frame_transitive(
    before: &SchedulerState,
    middle: &SchedulerState,
    after: &SchedulerState,
)
    requires
        mutation::work_update_preserves_other_state(before, middle),
        mutation::work_update_preserves_other_state(middle, after),
    ensures mutation::work_update_preserves_other_state(before, after),
{
    reveal(mutation::work_update_preserves_other_state);
}

/// Composes the unconditional state frame and retained work-identity layout.
pub(super) proof fn compose_layout_and_frame(
    before: &SchedulerState,
    middle: &SchedulerState,
    after: &SchedulerState,
)
    requires
        mutation::work_update_preserves_other_state(before, middle),
        mutation::work_update_preserves_other_state(middle, after),
        work_id_layout_matches(before.spec_work(), middle.spec_work()),
        work_id_layout_matches(middle.spec_work(), after.spec_work()),
    ensures
        mutation::work_update_preserves_other_state(before, after),
        work_id_layout_matches(before.spec_work(), after.spec_work()),
{
    frame_transitive(before, middle, after);
    reveal(work_id_layout_matches);
    assert forall |index: int| #![trigger before.spec_work()[index]]
        0 <= index < before.spec_work().len() implies
            after.spec_work()[index].spec_definition().spec_id()
                == before.spec_work()[index].spec_definition().spec_id() by {
        assert(middle.spec_work()[index].spec_definition().spec_id()
            == before.spec_work()[index].spec_definition().spec_id());
    }
}

/// Lifecycle-only work changes retain every canonical collection order.
pub(super) proof fn collection_order_preserved(
    before: &SchedulerState,
    after: &SchedulerState,
)
    requires
        work_id_layout_matches(before.spec_work(), after.spec_work()),
        mutation::work_update_preserves_other_state(before, after),
    ensures before.spec_collections_ordered() ==> after.spec_collections_ordered(),
{
    reveal(work_id_layout_matches);
    reveal(mutation::work_update_preserves_other_state);
    reveal(SchedulerState::spec_collections_ordered);
    reveal(SchedulerState::spec_work_ordered);
    reveal(SchedulerState::work_records_ordered);
    if before.spec_collections_ordered() {
        assert forall |left: int, right: int|
            0 <= left < right < after.spec_work().len() implies
                after.spec_work()[left].spec_definition().spec_id().spec_precedes(
                    &after.spec_work()[right].spec_definition().spec_id(),
                ) by {
            assert(before.spec_work()[left].spec_definition().spec_id()
                == after.spec_work()[left].spec_definition().spec_id());
            assert(before.spec_work()[right].spec_definition().spec_id()
                == after.spec_work()[right].spec_definition().spec_id());
        }
    }
}

} // verus!
