//! Reservation membership scan used by one cancellation update.

#[cfg(verus_only)]
use super::has_active_reservation;
use crate::{SchedulerReservation, WorkId};
use vstd::prelude::*;

verus! {

pub(super) fn is_active(reservations: &[SchedulerReservation], id: WorkId) -> (active: bool)
    ensures active == has_active_reservation(reservations@, id),
{
    let mut index = 0;
    while index < reservations.len()
        invariant
            index <= reservations.len(),
            forall |prior: int| #![trigger reservations@[prior]] 0 <= prior < index ==>
                reservations@[prior].spec_work_id() != id,
        decreases reservations.len() - index,
    {
        if reservations[index].work_id().same(&id) {
            proof {
                assert(reservations@[index as int].spec_work_id() == id);
                assert(has_active_reservation(reservations@, id));
            }
            return true;
        }
        proof { assert(reservations@[index as int].spec_work_id() != id); }
        index += 1;
    }
    proof {
        assert forall |at: int| #![trigger reservations@[at]] 0 <= at < reservations.len()
            implies reservations@[at].spec_work_id() != id by {
            assert(at < index);
        }
        reveal(has_active_reservation);
    }
    false
}

} // verus!
