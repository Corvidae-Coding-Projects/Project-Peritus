//! Reducer-readiness witness after retaining one selected reservation.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkRecord};

verus! {

pub proof fn inserted_state_is_ready(
    state: &SchedulerState,
    before_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    before_used: Seq<DispatchId>,
    work_index: int,
    dispatch_id: DispatchId,
    stored: SchedulerReservation,
)
    requires
        state.spec_reservation_invariant(),
        crate::verified::reservations_bind_active_work(before_work, before_reservations),
        crate::verified::reservations_match_work_phases(before_work, before_reservations),
        crate::verified::active_work_has_reservations(before_work, before_reservations),
        crate::verified::reservations_are_retained_dispatches(
            before_reservations,
            before_used,
        ),
        0 <= work_index < before_work.len(),
        before_work[work_index].spec_phase() == crate::WorkPhase::Queued,
        before_work.len() == state.spec_work().len(),
        state.spec_work()[work_index].spec_phase() == crate::WorkPhase::Reserved,
        WorkRecord::reservation_subject_equivalent(
            &before_work[work_index],
            &state.spec_work()[work_index],
        ),
        forall |other: int| #![auto]
            0 <= other < before_work.len() && other != work_index ==>
                before_work[other] == state.spec_work()[other],
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() ==>
                before_reservations[index].spec_work_id()
                    != before_work[work_index].spec_definition().spec_id(),
        exists |at: int| #![auto]
            0 <= at <= before_reservations.len()
                && state.spec_reservations() == before_reservations.insert(at, stored),
        exists |at: int| #![auto]
            0 <= at <= before_used.len()
                && state.spec_used_dispatches() == before_used.insert(at, dispatch_id),
        stored.spec_dispatch_id() == dispatch_id,
        stored.spec_work_id()
            == state.spec_work()[work_index].spec_definition().spec_id(),
        !stored.spec_started(),
    ensures state.spec_reservation_reducer_ready(),
{
    let reservation_index = choose |at: int| #![auto]
        0 <= at <= before_reservations.len()
            && state.spec_reservations() == before_reservations.insert(at, stored);
    let used_index = choose |at: int| #![auto]
        0 <= at <= before_used.len()
            && state.spec_used_dispatches() == before_used.insert(at, dispatch_id);
    let ghost inserted = state.spec_reservations()[reservation_index];
    assert(inserted == stored);
    assert(inserted.spec_dispatch_id() == dispatch_id);
    assert(state.spec_reservations()
        == before_reservations.insert(reservation_index, inserted));
    assert(state.spec_used_dispatches()
        == before_used.insert(used_index, inserted.spec_dispatch_id()));
    crate::verified::selected_insertion_establishes_relations(
        before_work,
        state.spec_work(),
        before_reservations,
        state.spec_reservations(),
        before_used,
        state.spec_used_dispatches(),
        inserted,
        work_index,
        reservation_index,
        used_index,
    );
    crate::verified::selected_insertion_establishes_phase(
        before_work,
        state.spec_work(),
        before_reservations,
        state.spec_reservations(),
        inserted,
        work_index,
        reservation_index,
    );
    assert(state.spec_reservation_reducer_ready());
}

} // verus!
