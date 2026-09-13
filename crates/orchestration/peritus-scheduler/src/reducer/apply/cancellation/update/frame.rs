//! Checked identity and unrelated-record preservation across cancellation steps.

use super::{
    cancellation_step_matches, has_active_reservation, target_exists, work_id_layout_matches,
};
use crate::state::mutation;
use crate::{SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkRecord, WorkTerminal};
use vstd::prelude::*;

verus! {

pub(super) proof fn phase_update_preserves_ids(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    phase: WorkPhase,
    found: bool,
)
    requires mutation::work_phase_update_matches(before, after, id, phase, found),
    ensures
        before.len() == after.len(),
        forall |index: int| #![trigger before[index]] 0 <= index < before.len() ==>
            after[index].spec_definition().spec_id()
                == before[index].spec_definition().spec_id(),
{
    reveal(mutation::work_phase_update_matches);
    if found {
        let changed = choose |changed: int| #![trigger before[changed]] {
            &&& 0 <= changed < before.len()
            &&& mutation::work_record_update_matches(
                before[changed],
                after[changed],
                id,
                phase,
            )
            &&& after[changed].spec_retry_cause() == before[changed].spec_retry_cause()
            &&& after[changed].spec_terminal() == before[changed].spec_terminal()
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != changed ==>
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
    } else {
        assert(after == before);
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
    ensures
        before.len() == after.len(),
        forall |index: int| #![trigger before[index]] 0 <= index < before.len() ==>
            after[index].spec_definition().spec_id()
                == before[index].spec_definition().spec_id(),
{
    reveal(mutation::work_terminal_update_matches);
    if found {
        let changed = choose |changed: int| #![trigger before[changed]] {
            &&& 0 <= changed < before.len()
            &&& mutation::work_record_update_matches(
                before[changed],
                after[changed],
                id,
                WorkPhase::Terminal,
            )
            &&& after[changed].spec_retry_cause().is_none()
            &&& after[changed].spec_terminal() == Some(terminal)
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != changed ==>
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
    } else {
        assert(after == before);
    }
}

pub(super) proof fn cancellation_step_preserves_ids(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
)
    requires cancellation_step_matches(before, after, reservations, id),
    ensures
        work_id_layout_matches(before, after),
        before.len() == after.len(),
        forall |index: int| #![trigger before[index]] 0 <= index < before.len() ==>
            after[index].spec_definition().spec_id()
                == before[index].spec_definition().spec_id(),
{
    reveal(cancellation_step_matches);
    if has_active_reservation(reservations, id) {
        phase_update_preserves_ids(before, after, id, WorkPhase::Cancelling, true);
    } else {
        terminal_update_preserves_ids(before, after, id, WorkTerminal::Cancelled, true);
    }
    reveal(work_id_layout_matches);
}

pub(super) proof fn cancellation_step_target_exists(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
)
    requires cancellation_step_matches(before, after, reservations, id),
    ensures target_exists(before, id),
{
    reveal(cancellation_step_matches);
    if has_active_reservation(reservations, id) {
        reveal(mutation::work_phase_update_matches);
        let retained = choose |retained: int| #![trigger before[retained]] {
            &&& 0 <= retained < before.len()
            &&& mutation::work_record_update_matches(
                before[retained],
                after[retained],
                id,
                WorkPhase::Cancelling,
            )
        };
        reveal(mutation::work_record_update_matches);
        reveal(target_exists);
        assert(before[retained].spec_definition().spec_id() == id);
    } else {
        reveal(mutation::work_terminal_update_matches);
        let retained = choose |retained: int| #![trigger before[retained]] {
            &&& 0 <= retained < before.len()
            &&& mutation::work_record_update_matches(
                before[retained],
                after[retained],
                id,
                WorkPhase::Terminal,
            )
        };
        reveal(mutation::work_record_update_matches);
        reveal(target_exists);
        assert(before[retained].spec_definition().spec_id() == id);
    }
}

pub(super) proof fn target_existence_preserved(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
)
    requires work_id_layout_matches(before, after),
    ensures target_exists(before, id) == target_exists(after, id),
{
    reveal(work_id_layout_matches);
    reveal(target_exists);
    if target_exists(before, id) {
        let retained = choose |retained: int| #![trigger before[retained]]
            0 <= retained < before.len()
                && before[retained].spec_definition().spec_id() == id;
        assert(after[retained].spec_definition().spec_id() == id);
    }
    if target_exists(after, id) {
        let retained = choose |retained: int| #![trigger after[retained]]
            0 <= retained < after.len()
                && after[retained].spec_definition().spec_id() == id;
        assert(0 <= retained < before.len());
        assert(before[retained].spec_definition().spec_id() == id);
    }
}

pub(super) proof fn phase_update_preserves_other_record(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    phase: WorkPhase,
    index: int,
)
    requires
        mutation::work_phase_update_matches(before, after, id, phase, true),
        0 <= index < before.len(),
        before[index].spec_definition().spec_id() != id,
    ensures after[index] == before[index],
{
    reveal(mutation::work_phase_update_matches);
    let changed = choose |changed: int| #![trigger before[changed]] {
        &&& 0 <= changed < before.len()
        &&& mutation::work_record_update_matches(before[changed], after[changed], id, phase)
        &&& after[changed].spec_retry_cause() == before[changed].spec_retry_cause()
        &&& after[changed].spec_terminal() == before[changed].spec_terminal()
        &&& forall |other: int| #![auto]
            0 <= other < before.len() && other != changed ==>
                after[other] == before[other]
    };
    reveal(mutation::work_record_update_matches);
    assert(changed != index);
}

pub(super) proof fn terminal_update_preserves_other_record(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    terminal: WorkTerminal,
    index: int,
)
    requires
        mutation::work_terminal_update_matches(before, after, id, terminal, true),
        0 <= index < before.len(),
        before[index].spec_definition().spec_id() != id,
    ensures after[index] == before[index],
{
    reveal(mutation::work_terminal_update_matches);
    let changed = choose |changed: int| #![trigger before[changed]] {
        &&& 0 <= changed < before.len()
        &&& mutation::work_record_update_matches(
            before[changed],
            after[changed],
            id,
            WorkPhase::Terminal,
        )
        &&& after[changed].spec_retry_cause().is_none()
        &&& after[changed].spec_terminal() == Some(terminal)
        &&& forall |other: int| #![auto]
            0 <= other < before.len() && other != changed ==>
                after[other] == before[other]
    };
    reveal(mutation::work_record_update_matches);
    assert(changed != index);
}

pub(super) proof fn cancellation_step_preserves_other_record(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
    index: int,
)
    requires
        cancellation_step_matches(before, after, reservations, id),
        0 <= index < before.len(),
        before[index].spec_definition().spec_id() != id,
    ensures after[index] == before[index],
{
    reveal(cancellation_step_matches);
    if has_active_reservation(reservations, id) {
        phase_update_preserves_other_record(
            before,
            after,
            id,
            WorkPhase::Cancelling,
            index,
        );
    } else {
        terminal_update_preserves_other_record(
            before,
            after,
            id,
            WorkTerminal::Cancelled,
            index,
        );
    }
}

/// Each cancellation phase agrees exactly with whether ownership is retained.
pub(super) proof fn cancellation_update_admissible(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
)
    requires target_exists(work, id),
    ensures crate::verified::work_phase_update_admissible(
        work, reservations, id,
        if has_active_reservation(reservations, id) {
            WorkPhase::Cancelling
        } else {
            WorkPhase::Terminal
        },
    ),
{
    reveal(target_exists);
    reveal(has_active_reservation);
    reveal(crate::verified::work_phase_update_admissible);
    reveal(crate::verified::work_phase_retains_reservation);
}

/// Lifecycle-only changes preserve the canonical order needed by subsequent commands.
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
