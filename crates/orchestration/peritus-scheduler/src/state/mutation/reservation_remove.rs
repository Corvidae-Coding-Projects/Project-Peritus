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

/// Relates the returned value to the exact reservation removed from the old sequence.
pub open spec fn exact_reservation_removal_matches(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
    id: DispatchId,
    removed: Option<SchedulerReservation>,
) -> bool {
    match removed {
        Some(reservation) => exists |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& before[index].spec_dispatch_id() == id
            &&& reservation == before[index]
            &&& after == before.remove(index)
        },
        None => {
            &&& after == before
            &&& forall |index: int| #![trigger before[index]]
                0 <= index < before.len() ==>
                    before[index].spec_dispatch_id() != id
        },
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
        final(state).spec_sequence() == old(state).spec_sequence(),
        final(state).spec_last_event_id() == old(state).spec_last_event_id(),
        final(state).spec_state_digest() == old(state).spec_state_digest(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        final(state).spec_enqueue_ordinal() == old(state).spec_enqueue_ordinal(),
        final(state).spec_dispatch_ordinal() == old(state).spec_dispatch_ordinal(),
        final(state).spec_used_commands() == old(state).spec_used_commands(),
        final(state).spec_terminal() == old(state).spec_terminal(),
        old(state).spec_reservations_ordered() ==> final(state).spec_reservations_ordered(),
        reservation_removal_matches(
            old(state).spec_reservations(),
            final(state).spec_reservations(),
            id,
            result.is_some(),
        ),
        exact_reservation_removal_matches(
            old(state).spec_reservations(),
            final(state).spec_reservations(),
            id,
            result,
        ),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost before = state.spec_reservations();
    let ghost was_ordered = state.spec_reservations_ordered();
    let mut index = 0;
    while index < state.reservations.len()
        invariant
            index <= state.reservations@.len(),
            state.spec_reservations() == before,
            before == old(state).spec_reservations(),
            was_ordered == old(state).spec_reservations_ordered(),
            was_ordered ==> state.spec_reservations_ordered(),
            forall |prior: int| #![auto]
                0 <= prior < index ==> before[prior].spec_dispatch_id() != id,
            had_invariant == old(state).spec_reservation_invariant(),
            had_invariant ==> state.spec_reservation_invariant(),
        decreases state.reservations@.len() - index,
    {
        if state.reservations[index].dispatch_id().same(&id) {
            proof {
                assert(0 <= (index as int) && (index as int) < before.len());
                if was_ordered {
                    SchedulerState::reservation_removal_ordered(before, index as int);
                }
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
                assert(removed == before[index as int]);
                assert(state.spec_reservations() == before.remove(index as int));
                reveal(reservation_removal_matches);
                reveal(exact_reservation_removal_matches);
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
        reveal(exact_reservation_removal_matches);
    }
    None
}

} // verus!
