//! Verified preparation of one exact selected reservation.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState};

verus! {

/// Exact bounds needed to begin a selected attempt and allocate its ordinal.
pub open spec fn preparation_available(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& 0 <= work_index < state.spec_work().len()
    &&& 0 <= worker_index < state.spec_workers().len()
    &&& state.spec_work()[work_index].spec_attempts_started() < u16::MAX
    &&& state.spec_work()[work_index].spec_attempts_started()
        < state.spec_work()[work_index]
            .spec_definition().spec_maximum_attempts().spec_value()
    &&& state.spec_dispatch_ordinal() < u64::MAX
}

/// Exact state and reservation relation produced by attempt preparation.
pub open spec fn preparation_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
    reservation: &SchedulerReservation,
) -> bool {
    &&& after.spec_binding().spec_limits() == before.spec_binding().spec_limits()
    &&& after.spec_binding().spec_capacity().spec_entries()
        == before.spec_binding().spec_capacity().spec_entries()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_work().len() == before.spec_work().len()
    &&& forall |other: int| #![auto]
        0 <= other < before.spec_work().len() && other != work_index ==>
            after.spec_work()[other] == before.spec_work()[other]
    &&& crate::WorkRecord::reservation_subject_equivalent(
        &before.spec_work()[work_index],
        &after.spec_work()[work_index],
    )
    &&& after.spec_work()[work_index].spec_phase() == crate::WorkPhase::Reserved
    &&& reservation.spec_work_id()
        == before.spec_work()[work_index].spec_definition().spec_id()
    &&& reservation.spec_dispatch_id() == dispatch_id
    &&& reservation.spec_worker_id()
        == before.spec_workers()[worker_index].spec_descriptor().spec_id()
    &&& crate::identity::actor_ids_match(
        reservation.spec_owner(),
        before.spec_work()[work_index].spec_definition().spec_owner(),
    )
    &&& reservation.spec_revision()
        == before.spec_work()[work_index].spec_definition().spec_revision()
    &&& reservation.spec_resources().spec_entries()
        == before.spec_work()[work_index]
            .spec_definition().spec_request().spec_entries()
    &&& reservation.spec_dispatch_token() == dispatch_token
    &&& reservation.spec_attempt().spec_value()
        == after.spec_work()[work_index].spec_attempts_started()
    &&& !reservation.spec_started()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal() + 1
}

pub fn selected_reservation(
    state: &mut SchedulerState,
    work_index: usize,
    worker_index: usize,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
) -> (result: Option<SchedulerReservation>)
    ensures
        result.is_some() <==> preparation_available(
            old(state), work_index as int, worker_index as int,
        ),
        match result {
            Some(reservation) => preparation_matches(
                old(state), final(state), work_index as int, worker_index as int,
                dispatch_id, dispatch_token, &reservation,
            ),
            None => true,
        },
        old(state).spec_reservation_invariant()
                && preparation_available(old(state), work_index as int, worker_index as int)
                && (forall |reservation_index: int| #![auto]
                    0 <= reservation_index < old(state).spec_reservations().len() ==>
                        old(state).spec_reservations()[reservation_index].spec_work_id()
                            != old(state).spec_work()[work_index as int]
                                .spec_definition().spec_id())
            ==> final(state).spec_reservation_invariant(),
{
    if work_index >= state.work().len() || worker_index >= state.workers().len() {
        return None;
    }
    let work = &state.work()[work_index];
    if work.attempts_started() == u16::MAX
        || work.attempts_started() >= work.spec().maximum_attempts().get()
        || state.dispatch_ordinal() == u64::MAX
    {
        return None;
    }
    let work_id = work.spec().id();
    let owner = work.spec().owner();
    let revision = work.spec().revision();
    let resources = work.spec().request().clone();
    let worker_id = state.workers()[worker_index].descriptor().id();
    let ghost before = state;
    let attempt = super::super::begin_work_attempt_at(state, work_index);
    proof { assert(attempt.is_some()); }
    let Some(attempt) = attempt else {
        proof { assert(false); }
        return None;
    };
    let incremented = super::super::increment_dispatch_ordinal(state);
    proof { assert(incremented); }
    if !incremented {
        proof { assert(false); }
        return None;
    }
    let reservation = SchedulerReservation::new(
        work_id,
        dispatch_id,
        worker_id,
        owner,
        attempt,
        revision,
        resources,
        dispatch_token,
    );
    proof {
        reveal(preparation_matches);
        assert(preparation_matches(
            old(state), state, work_index as int, worker_index as int,
            dispatch_id, dispatch_token, &reservation,
        ));
        assert(before == old(state));
    }
    Some(reservation)
}

} // verus!
