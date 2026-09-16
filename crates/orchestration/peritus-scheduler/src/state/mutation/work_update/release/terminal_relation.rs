//! Exact state relation for successful terminal reservation release.

use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkTerminal};

verus! {

/// Relates every scheduler field outside the released reservation and updated work.
pub open spec fn release_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
}

/// Exact successful effect of releasing one reservation into a terminal work state.
pub open spec fn terminal_release_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    terminal: WorkTerminal,
    removed: &SchedulerReservation,
) -> bool {
    &&& super::super::super::reservation_remove::exact_reservation_removal_matches(
        before.spec_reservations(),
        after.spec_reservations(),
        dispatch_id,
        Some(*removed),
    )
    &&& removed.spec_dispatch_id() == dispatch_id
    &&& removed.spec_work_id() == work_id
    &&& super::super::work_terminal_update_matches(
        before.spec_work(), after.spec_work(), work_id, terminal, true,
    )
    &&& release_preserves_other_state(before, after)
}

/// Exact terminal release preserves every canonical binary-search ordering.
pub(super) proof fn terminal_release_preserves_collections_order(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    terminal: WorkTerminal,
    removed: &SchedulerReservation,
)
    requires
        before.spec_collections_ordered(),
        terminal_release_matches(before, after, dispatch_id, work_id, terminal, removed),
    ensures after.spec_collections_ordered(),
{
    reveal(terminal_release_matches);
    reveal(release_preserves_other_state);
    reveal(SchedulerState::spec_collections_ordered);
    reveal(SchedulerState::spec_workers_ordered);
    reveal(SchedulerState::spec_work_ordered);
    reveal(SchedulerState::spec_reservations_ordered);
    reveal(SchedulerState::spec_used_dispatches_ordered);

    reveal(super::super::super::reservation_remove::exact_reservation_removal_matches);
    let reservation_index = choose |index: int| #![trigger before.spec_reservations()[index]] {
        &&& 0 <= index < before.spec_reservations().len()
        &&& before.spec_reservations()[index].spec_dispatch_id() == dispatch_id
        &&& *removed == before.spec_reservations()[index]
        &&& after.spec_reservations() == before.spec_reservations().remove(index)
    };
    SchedulerState::reservation_removal_ordered(
        before.spec_reservations(), reservation_index,
    );

    reveal(super::super::work_terminal_update_matches);
    let work_index = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& super::super::work_record_update_matches(
            before.spec_work()[index], after.spec_work()[index], work_id,
            crate::WorkPhase::Terminal,
        )
        &&& after.spec_work()[index].spec_retry_cause().is_none()
        &&& after.spec_work()[index].spec_terminal() == Some(terminal)
        &&& forall |other: int| #![auto]
            0 <= other < before.spec_work().len() && other != index ==>
                after.spec_work()[other] == before.spec_work()[other]
    };
    reveal(super::super::work_record_update_matches);
    crate::WorkRecord::lifecycle_update_fields(
        &before.spec_work()[work_index],
        &after.spec_work()[work_index],
    );
    assert forall |index: int| 0 <= index < before.spec_work().len() implies
        after.spec_work()[index].spec_definition().spec_id()
            == before.spec_work()[index].spec_definition().spec_id() by {
        if index != work_index {
            assert(after.spec_work()[index] == before.spec_work()[index]);
        }
    }
    reveal(SchedulerState::work_records_ordered);
    assert(SchedulerState::work_records_ordered(after.spec_work())) by {
        assert forall |left: int, right: int|
            0 <= left < right < after.spec_work().len() implies
                after.spec_work()[left].spec_definition().spec_id().spec_precedes(
                    &after.spec_work()[right].spec_definition().spec_id()) by {
            assert(before.spec_work()[left].spec_definition().spec_id().spec_precedes(
                &before.spec_work()[right].spec_definition().spec_id()));
        }
    }
}

} // verus!
