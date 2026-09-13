//! Exact live-reservation removal used by combined release transitions.

use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState};

verus! {

/// Relates a live-reservation sequence to the exact result of one identity removal.
pub open spec fn reservation_removal_matches(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
    id: DispatchId,
    found: bool,
) -> bool {
    if found {
        exists |index: int| #![trigger before[index]]
            0 <= index < before.len()
                && before[index].spec_dispatch_id() == id
                && after == before.remove(index)
    } else {
        &&& after == before
        &&& forall |index: int| #![trigger before[index]]
            0 <= index < before.len() ==>
                before[index].spec_dispatch_id() != id
    }
}

pub fn remove_reservation(
    state: &mut SchedulerState,
    id: DispatchId,
) -> (result: Option<SchedulerReservation>)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        reservation_removal_matches(
            old(state).spec_reservations(),
            final(state).spec_reservations(),
            id,
            result.is_some(),
        ),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost before = state.spec_reservations();
    let mut index = 0;
    while index < state.reservations.len()
        invariant
            index <= state.reservations@.len(),
            state.spec_reservations() == before,
            before == old(state).spec_reservations(),
            forall |prior: int| #![auto]
                0 <= prior < index ==> before[prior].spec_dispatch_id() != id,
            had_invariant == old(state).spec_reservation_invariant(),
            had_invariant ==> state.spec_reservation_invariant(),
        decreases state.reservations@.len() - index,
    {
        if state.reservations[index].dispatch_id().same(&id) {
            proof {
                assert(0 <= (index as int) && (index as int) < before.len());
                if state.spec_reservation_invariant() {
                    crate::verified::actual_reservation_removal_preserves(
                        state.spec_binding(),
                        state.spec_workers(),
                        state.spec_work(),
                        state.spec_reservations(),
                        index as int,
                    );
                }
            }
            let removed = state.reservations.remove(index);
            proof {
                assert(state.spec_reservations() == before.remove(index as int));
                reveal(reservation_removal_matches);
                assert(reservation_removal_matches(
                    before,
                    state.spec_reservations(),
                    id,
                    true,
                ));
                assert(reservation_removal_matches(
                    old(state).spec_reservations(),
                    state.spec_reservations(),
                    id,
                    true,
                ));
                if had_invariant {
                    assert(state.spec_reservation_invariant());
                }
            }
            return Some(removed);
        }
        proof {
            assert(before[index as int].spec_dispatch_id() != id);
        }
        index += 1;
    }
    proof {
        assert(state.spec_reservations() == before);
        assert forall |reservation_index: int| #![trigger before[reservation_index]]
            0 <= reservation_index < before.len() implies
                before[reservation_index].spec_dispatch_id() != id by {
            assert(reservation_index < index);
        }
        reveal(reservation_removal_matches);
        assert(reservation_removal_matches(before, state.spec_reservations(), id, false));
        assert(reservation_removal_matches(
            old(state).spec_reservations(),
            state.spec_reservations(),
            id,
            false,
        ));
    }
    None
}

} // verus!
