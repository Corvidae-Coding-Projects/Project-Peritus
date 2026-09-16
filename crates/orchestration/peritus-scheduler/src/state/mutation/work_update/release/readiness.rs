//! Reservation-readiness proof for one exact release and work update.

use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase};

verus! {

pub(super) proof fn establish_released_state_ready(
    state: &SchedulerState,
    before_work: Seq<crate::WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_removal: Seq<SchedulerReservation>,
    used_dispatches: Seq<DispatchId>,
    dispatch_id: DispatchId,
    work_id: WorkId,
    target_phase: WorkPhase,
    removed: SchedulerReservation,
)
    requires
        state.spec_reservation_invariant(),
        state.spec_reservations() == after_removal,
        state.spec_used_dispatches() == used_dispatches,
        crate::verified::work_identities_unique(before_work),
        crate::verified::reservation_identities_unique(before_reservations),
        crate::verified::reservations_match_work_phases(before_work, before_reservations),
        crate::verified::active_work_has_reservations(before_work, before_reservations),
        crate::verified::reservations_are_retained_dispatches(
            before_reservations,
            used_dispatches,
        ),
        exists |reservation_index: int|
            #![trigger before_reservations[reservation_index]]
            0 <= reservation_index < before_reservations.len()
                && before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
                && before_reservations[reservation_index].spec_work_id() == work_id,
        super::super::super::reservation_remove::exact_reservation_removal_matches(
            before_reservations,
            after_removal,
            dispatch_id,
            Some(removed),
        ),
        super::super::work_update_matches(
            before_work,
            state.spec_work(),
            work_id,
            target_phase,
            true,
        ),
        !crate::verified::work_phase_retains_reservation(target_phase),
    ensures
        state.spec_reservation_reducer_ready(),
        removed.spec_dispatch_id() == dispatch_id,
        removed.spec_work_id() == work_id,
{
    let target_index = choose |reservation_index: int|
        #![trigger before_reservations[reservation_index]]
        0 <= reservation_index < before_reservations.len()
            && before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
            && before_reservations[reservation_index].spec_work_id() == work_id;
    reveal(super::super::super::reservation_remove::exact_reservation_removal_matches);
    let removed_index = choose |reservation_index: int|
        #![trigger before_reservations[reservation_index]] {
            &&& 0 <= reservation_index < before_reservations.len()
            &&& before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
            &&& removed == before_reservations[reservation_index]
            &&& after_removal == before_reservations.remove(reservation_index)
        };
    reveal(crate::verified::reservation_identities_unique);
    assert(removed_index == target_index) by {
        if removed_index != target_index {
            assert(before_reservations[removed_index].spec_dispatch_id()
                != before_reservations[target_index].spec_dispatch_id());
            assert(false);
        }
    }
    reveal(super::super::work_update_matches);
    let work_index = choose |index: int| #![trigger before_work[index]] {
        &&& 0 <= index < before_work.len()
        &&& super::super::work_record_update_matches(
            before_work[index], state.spec_work()[index], work_id, target_phase,
        )
        &&& forall |other: int| #![auto]
            0 <= other < before_work.len() && other != index ==>
                state.spec_work()[other] == before_work[other]
    };
    reveal(super::super::work_record_update_matches);
    assert(before_reservations[removed_index].spec_work_id()
        == before_work[work_index].spec_definition().spec_id());
    crate::verified::release_preserves_relations(
        before_work,
        state.spec_work(),
        before_reservations,
        state.spec_reservations(),
        used_dispatches,
        work_index,
        removed_index,
    );
    reveal(SchedulerState::spec_reservation_reducer_ready);
}

} // verus!
