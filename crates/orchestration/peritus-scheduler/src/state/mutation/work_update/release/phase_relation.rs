//! Exact state relation for successful phase reservation release.

use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase};

use super::terminal_relation::release_preserves_other_state;

verus! {

/// Exact successful effect of releasing one reservation into a supplied work phase.
pub open spec fn phase_release_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    phase: WorkPhase,
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
    &&& super::super::work_phase_update_matches(
        before.spec_work(), after.spec_work(), work_id, phase, true,
    )
    &&& release_preserves_other_state(before, after)
}

/// Exact phase release preserves every canonical binary-search ordering.
pub(super) proof fn phase_release_preserves_collections_order(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    phase: WorkPhase,
    removed: &SchedulerReservation,
)
    requires
        before.spec_collections_ordered(),
        phase_release_matches(before, after, dispatch_id, work_id, phase, removed),
    ensures after.spec_collections_ordered(),
{
    reveal(phase_release_matches);
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

    reveal(super::super::work_phase_update_matches);
    let work_index = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& super::super::work_record_update_matches(
            before.spec_work()[index], after.spec_work()[index], work_id, phase,
        )
        &&& after.spec_work()[index].spec_retry_cause()
            == before.spec_work()[index].spec_retry_cause()
        &&& after.spec_work()[index].spec_terminal()
            == before.spec_work()[index].spec_terminal()
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
