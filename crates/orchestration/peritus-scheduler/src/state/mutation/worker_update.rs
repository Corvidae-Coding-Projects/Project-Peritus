//! Verified reservation-stable worker lifecycle mutations.

use vstd::prelude::*;

use crate::{SchedulerState, WorkerId, WorkerPhase};

verus! {

/// Exact worker-sequence effect of one phase update attempt.
pub open spec fn worker_phase_update_matches(
    before: Seq<crate::WorkerRecord>,
    after: Seq<crate::WorkerRecord>,
    id: WorkerId,
    phase: WorkerPhase,
    found: bool,
) -> bool {
    &&& before.len() == after.len()
    &&& if found {
        exists |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& before[index].spec_descriptor().spec_id() == id
            &&& crate::WorkerRecord::reservation_owner_equivalent(
                &before[index],
                &after[index],
            )
            &&& after[index].spec_phase() == phase
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != index ==>
                    after[other] == before[other]
        }
    } else {
        &&& after == before
        &&& forall |index: int| #![trigger before[index]]
            0 <= index < before.len() ==>
                before[index].spec_descriptor().spec_id() != id
    }
}

fn set_worker_phase_at(
    state: &mut SchedulerState,
    index: usize,
    _id: WorkerId,
    phase: WorkerPhase,
)
    requires
        index < state.spec_workers().len(),
        state.spec_workers()[index as int].spec_descriptor().spec_id() == _id,
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        worker_phase_update_matches(
            old(state).spec_workers(),
            final(state).spec_workers(),
            _id,
            phase,
            true,
        ),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
{
    let ghost before = state.spec_workers();
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    state.workers[index].set_phase(phase);
    proof {
        assert(before.len() == state.spec_workers().len());
        assert(crate::WorkerRecord::reservation_owner_equivalent(
            &before[index as int],
            &state.spec_workers()[index as int],
        ));
        assert(state.spec_workers()[index as int].spec_phase() == phase);
        assert forall |other: int| #![auto]
            0 <= other < before.len() && other != index
                implies before[other] == state.spec_workers()[other] by {
        }
        if had_invariant {
            crate::verified::actual_reservation_worker_update_preserves(
                state.spec_binding(),
                before,
                state.spec_workers(),
                state.spec_work(),
                state.spec_reservations(),
                index as int,
            );
        }
        if was_ready {
            reveal(SchedulerState::spec_reservation_reducer_ready);
        }
        reveal(worker_phase_update_matches);
        assert(exists |found_at: int| #![trigger before[found_at]] {
            &&& 0 <= found_at < before.len()
            &&& before[found_at].spec_descriptor().spec_id() == _id
            &&& crate::WorkerRecord::reservation_owner_equivalent(
                &before[found_at],
                &state.spec_workers()[found_at],
            )
            &&& state.spec_workers()[found_at].spec_phase() == phase
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != found_at ==>
                    state.spec_workers()[other] == before[other]
        }) by {
            assert(0 <= (index as int) && (index as int) < before.len());
        }
    };
}

pub fn set_worker_phase(
    state: &mut SchedulerState,
    id: WorkerId,
    phase: WorkerPhase,
) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        worker_phase_update_matches(
            old(state).spec_workers(),
            final(state).spec_workers(),
            id,
            phase,
            found,
        ),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost binding = state.spec_binding();
    let ghost before = state.spec_workers();
    let ghost work = state.spec_work();
    let ghost reservations = state.spec_reservations();
    let ghost used_dispatches = state.spec_used_dispatches();
    if let Some(index) = state.worker_index(id) {
        proof {
            assert(before[index as int].spec_descriptor().spec_id() == id);
        }
        set_worker_phase_at(state, index, id, phase);
        proof {
            assert(before == old(state).spec_workers());
            assert(worker_phase_update_matches(
                old(state).spec_workers(),
                state.spec_workers(),
                id,
                phase,
                true,
            ));
        }
        return true;
    }
    // Valid scheduler states find retained workers above. Keep a complete fallback so this private
    // mutator retains its exact result contract even for an unvalidated decoded state.
    let mut index = 0;
    while index < state.workers.len()
        invariant
            index <= state.workers@.len(),
            had_invariant == old(state).spec_reservation_invariant(),
            had_invariant ==> state.spec_reservation_invariant(),
            was_ready == old(state).spec_reservation_reducer_ready(),
            was_ready ==> state.spec_reservation_reducer_ready(),
            state.spec_workers() == before,
            state.spec_binding() == binding,
            state.spec_work() == work,
            state.spec_reservations() == reservations,
            state.spec_used_dispatches() == used_dispatches,
            binding == old(state).spec_binding(),
            before == old(state).spec_workers(),
            work == old(state).spec_work(),
            reservations == old(state).spec_reservations(),
            used_dispatches == old(state).spec_used_dispatches(),
            forall |prior: int| #![auto]
                0 <= prior < index ==>
                    before[prior].spec_descriptor().spec_id() != id,
        decreases state.workers@.len() - index,
    {
        if state.workers[index].descriptor().id().same(&id) {
            proof {
                assert(before[index as int].spec_descriptor().spec_id() == id);
            }
            set_worker_phase_at(state, index, id, phase);
            proof {
                assert(before == old(state).spec_workers());
                assert(worker_phase_update_matches(
                    old(state).spec_workers(),
                    state.spec_workers(),
                    id,
                    phase,
                    true,
                ));
            }
            return true;
        }
        proof {
            assert(before[index as int].spec_descriptor().spec_id() != id);
        }
        index += 1;
    }
    proof {
        reveal(worker_phase_update_matches);
        assert(state.spec_workers() == before);
        assert forall |at: int| #![trigger before[at]]
            0 <= at < before.len() implies
                before[at].spec_descriptor().spec_id() != id by {
            assert(at < index);
        }
        assert(worker_phase_update_matches(
            before,
            state.spec_workers(),
            id,
            phase,
            false,
        ));
        assert(worker_phase_update_matches(
            old(state).spec_workers(),
            state.spec_workers(),
            id,
            phase,
            false,
        ));
    }
    false
}

} // verus!
