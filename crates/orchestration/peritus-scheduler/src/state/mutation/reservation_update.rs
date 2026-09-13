//! Verified reservation bookkeeping mutations.

use vstd::prelude::*;

use crate::{DispatchId, SchedulerState};

#[cfg(verus_only)]
use crate::{SchedulerReservation, WorkRecord, WorkerRecord};

verus! {

/// Relates a reservation sequence to the exact result of acknowledging one dispatch start.
pub open spec fn reservation_start_update_matches(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
    id: DispatchId,
    found: bool,
) -> bool {
    &&& before.len() == after.len()
    &&& if found {
        exists |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& before[index].spec_dispatch_id() == id
            &&& SchedulerReservation::invariant_equivalent(&before[index], &after[index])
            &&& after[index].spec_started()
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != index ==>
                    after[other] == before[other]
        }
    } else {
        &&& after == before
        &&& forall |index: int| #![trigger before[index]]
            0 <= index < before.len() ==> before[index].spec_dispatch_id() != id
    }
}

proof fn started_reservation_preserves_invariant(
    state: &SchedulerState,
    before: Seq<SchedulerReservation>,
    index: int,
)
    requires
        crate::verified::reservation_invariant_parts(
            state.spec_binding(),
            state.spec_workers(),
            state.spec_work(),
            before,
        ),
        crate::verified::reservations_bind_active_work(state.spec_work(), before),
        crate::verified::reservations_are_retained_dispatches(
            before,
            state.spec_used_dispatches(),
        ),
        before.len() == state.spec_reservations().len(),
        0 <= index < before.len(),
        forall |other: int| #![auto]
            0 <= other < before.len() ==>
                SchedulerReservation::invariant_equivalent(
                    &before[other],
                    &state.spec_reservations()[other],
                ),
    ensures state.spec_reservation_invariant(),
{
    assert forall |worker_index: int| #![auto]
        0 <= worker_index < state.spec_workers().len() implies
            WorkerRecord::reservation_owner_equivalent(
                &state.spec_workers()[worker_index],
                &state.spec_workers()[worker_index],
            ) by {
        WorkerRecord::reservation_owner_reflexive(&state.spec_workers()[worker_index]);
    }
    assert forall |work_index: int| #![auto]
        0 <= work_index < state.spec_work().len() implies
            WorkRecord::reservation_lifecycle_equivalent(
                &state.spec_work()[work_index],
                &state.spec_work()[work_index],
            ) by {
        WorkRecord::reservation_lifecycle_reflexive(&state.spec_work()[work_index]);
    }
    crate::verified::equivalent_state_preserves(
        state.spec_binding(),
        state.spec_binding(),
        state.spec_workers(),
        state.spec_workers(),
        state.spec_work(),
        state.spec_work(),
        before,
        state.spec_reservations(),
        state.spec_used_dispatches(),
        state.spec_used_dispatches(),
    );
}

proof fn establish_reservation_start_match(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
    id: DispatchId,
    index: int,
)
    requires
        before.len() == after.len(),
        0 <= index < before.len(),
        before[index].spec_dispatch_id() == id,
        SchedulerReservation::invariant_equivalent(&before[index], &after[index]),
        after[index].spec_started(),
        forall |other: int| #![auto]
            0 <= other < before.len() && other != index ==> after[other] == before[other],
    ensures reservation_start_update_matches(before, after, id, true),
{
    reveal(reservation_start_update_matches);
    assert(exists |found_at: int| #![trigger before[found_at]] {
        &&& 0 <= found_at < before.len()
        &&& before[found_at].spec_dispatch_id() == id
        &&& SchedulerReservation::invariant_equivalent(&before[found_at], &after[found_at])
        &&& after[found_at].spec_started()
        &&& forall |other: int| #![auto]
            0 <= other < before.len() && other != found_at ==>
                after[other] == before[other]
    }) by {
    }
}

fn mark_reservation_started_at(
    state: &mut SchedulerState,
    index: usize,
    _id: DispatchId,
)
    requires
        index < state.spec_reservations().len(),
        state.spec_reservations()[index as int].spec_dispatch_id() == _id,
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        reservation_start_update_matches(
            old(state).spec_reservations(),
            final(state).spec_reservations(),
            _id,
            true,
        ),
{
    let ghost before = state.spec_reservations();
    let ghost initially_ready = state.spec_reservation_reducer_ready();
    proof {
        if initially_ready {
            reveal(SchedulerState::spec_reservation_reducer_ready);
        }
    }
    state.reservations[index].mark_started();
    proof {
        assert(before.len() == state.spec_reservations().len());
        assert forall |other: int| #![auto]
            0 <= other < before.len() implies
                SchedulerReservation::invariant_equivalent(
                    &before[other],
                    &state.spec_reservations()[other],
                ) by {
            if other != index {
                assert(before[other] == state.spec_reservations()[other]);
                SchedulerReservation::invariant_reflexive(&before[other]);
            }
        }
        if initially_ready {
            started_reservation_preserves_invariant(state, before, index as int);
        }
        establish_reservation_start_match(
            before,
            state.spec_reservations(),
            _id,
            index as int,
        );
    };
}

/// Marks one live reservation started without changing capacity or ownership.
pub fn mark_reservation_started(state: &mut SchedulerState, id: DispatchId) -> (found: bool)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        reservation_start_update_matches(
            old(state).spec_reservations(),
            final(state).spec_reservations(),
            id,
            found,
        ),
{
    let ghost initially_ready = state.spec_reservation_reducer_ready();
    let ghost initial_phase = state.spec_phase();
    let ghost initial_binding = state.spec_binding();
    let ghost initial_workers = state.spec_workers();
    let ghost initial_work = state.spec_work();
    let ghost initial_reservations = state.spec_reservations();
    let ghost initial_used = state.spec_used_dispatches();
    let ghost initially_bound = crate::verified::reservations_bind_active_work(
        state.spec_work(),
        state.spec_reservations(),
    );
    let ghost initially_retained = crate::verified::reservations_are_retained_dispatches(
        state.spec_reservations(),
        state.spec_used_dispatches(),
    );
    proof {
        if initially_ready {
            reveal(SchedulerState::spec_reservation_reducer_ready);
            assert(initially_bound);
        }
    }
    let mut index = 0;
    while index < state.reservations.len()
        invariant
            index <= state.reservations@.len(),
            initially_ready == old(state).spec_reservation_reducer_ready(),
            initial_phase == old(state).spec_phase(),
            initial_binding == old(state).spec_binding(),
            initial_workers == old(state).spec_workers(),
            initial_work == old(state).spec_work(),
            initial_reservations == old(state).spec_reservations(),
            initial_used == old(state).spec_used_dispatches(),
            state.spec_phase() == initial_phase,
            state.spec_binding() == initial_binding,
            state.spec_workers() == initial_workers,
            state.spec_work() == initial_work,
            state.spec_reservations() == initial_reservations,
            state.spec_used_dispatches() == initial_used,
            initially_bound == crate::verified::reservations_bind_active_work(
                initial_work,
                initial_reservations,
            ),
            initially_retained == crate::verified::reservations_are_retained_dispatches(
                initial_reservations,
                initial_used,
            ),
            initially_ready ==> initially_bound,
            initially_ready ==> initially_retained,
            initially_ready ==> state.spec_reservation_invariant(),
            initially_ready ==> state.spec_reservation_reducer_ready(),
            forall |prior: int| #![auto]
                0 <= prior < index ==>
                    initial_reservations[prior].spec_dispatch_id() != id,
        decreases state.reservations@.len() - index,
    {
        if state.reservations[index].dispatch_id().same(&id) {
            proof {
                assert(initial_reservations[index as int].spec_dispatch_id() == id);
            }
            mark_reservation_started_at(state, index, id);
            return true;
        }
        proof {
            assert(initial_reservations[index as int].spec_dispatch_id() != id);
        }
        index += 1;
    }
    proof {
        assert forall |reservation_index: int|
            #![trigger initial_reservations[reservation_index]]
            0 <= reservation_index < initial_reservations.len()
            implies initial_reservations[reservation_index].spec_dispatch_id() != id by {
            assert(reservation_index < index);
        }
        reveal(reservation_start_update_matches);
        assert(reservation_start_update_matches(
            initial_reservations,
            state.spec_reservations(),
            id,
            false,
        ));
    }
    false
}

} // verus!
