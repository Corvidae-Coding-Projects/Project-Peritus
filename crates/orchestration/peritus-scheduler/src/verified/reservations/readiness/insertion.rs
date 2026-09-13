//! Lifecycle compatibility across inactive work admission.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{SchedulerReservation, WorkRecord};

#[cfg(verus_only)]
use super::{
    active_work_has_reservation, active_work_has_reservations, active_work_has_reservations_intro,
    reservation_has_matching_work, reservations_match_work_phases,
    reservations_match_work_phases_intro,
};

verus! {

/// Inserting inactive work cannot introduce or invalidate live ownership.
pub proof fn inactive_work_insertion_preserves_phase(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    record: WorkRecord,
    at: int,
)
    requires
        reservations_match_work_phases(work, reservations),
        active_work_has_reservations(work, reservations),
        0 <= at <= work.len(),
        !super::super::work_phase_retains_reservation(record.spec_phase()),
    ensures
        reservations_match_work_phases(work.insert(at, record), reservations),
        active_work_has_reservations(work.insert(at, record), reservations),
{
    let after = work.insert(at, record);
    work.insert_ensures(at, record);
    reveal(reservations_match_work_phases);
    assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
        implies reservation_has_matching_work(after, reservations[reservation_index]) by {
        let old_work = choose |candidate: int|
            #![trigger work[candidate].spec_definition().spec_id()]
            0 <= candidate < work.len()
                && super::reservation_matches_work_phase(
                    work[candidate],
                    reservations[reservation_index],
                );
        let new_work = if old_work < at { old_work } else { old_work + 1 };
        assert(0 <= new_work < after.len());
        assert(after[new_work] == work[old_work]);
        reveal(reservation_has_matching_work);
    }
    reservations_match_work_phases_intro(after, reservations);
    reveal(active_work_has_reservations);
    assert forall |work_index: int| #![trigger after[work_index]]
        0 <= work_index < after.len()
        implies active_work_has_reservation(after[work_index], reservations) by {
        reveal(active_work_has_reservation);
        if work_index == at {
            assert(after[work_index] == record);
        } else {
            let old_work = if work_index < at { work_index } else { work_index - 1 };
            assert(0 <= old_work < work.len());
            assert(after[work_index] == work[old_work]);
            assert(active_work_has_reservation(work[old_work], reservations));
        }
    }
    active_work_has_reservations_intro(after, reservations);
}

} // verus!
