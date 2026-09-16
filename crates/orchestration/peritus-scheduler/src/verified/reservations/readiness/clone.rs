//! Lifecycle compatibility transport across exact semantic sequence clones.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{SchedulerReservation, WorkRecord};

#[cfg(verus_only)]
use super::{
    active_work_has_reservation, active_work_has_reservations, active_work_has_reservations_intro,
    reservation_has_matching_work, reservation_matches_work_phase, reservations_match_work_phases,
    reservations_match_work_phases_intro,
};

verus! {

proof fn cloned_reservation_has_matching_work(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_reservations: Seq<SchedulerReservation>,
    reservation_index: int,
)
    requires
        before_work.len() == after_work.len(),
        forall |index: int| #![auto]
            0 <= index < before_work.len() ==>
                WorkRecord::reservation_lifecycle_equivalent(
                    &before_work[index],
                    &after_work[index],
                ),
        before_reservations.len() == after_reservations.len(),
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() ==>
                SchedulerReservation::clone_equivalent(
                    &before_reservations[index],
                    &after_reservations[index],
                ),
        reservations_match_work_phases(before_work, before_reservations),
        0 <= reservation_index < after_reservations.len(),
    ensures reservation_has_matching_work(
        after_work,
        after_reservations[reservation_index],
    ),
{
    assert(0 <= reservation_index < before_reservations.len());
    reveal(reservations_match_work_phases);
    let work_index = choose |candidate: int|
        #![trigger before_work[candidate].spec_definition().spec_id()]
        0 <= candidate < before_work.len()
            && reservation_matches_work_phase(
                before_work[candidate],
                before_reservations[reservation_index],
            );
    assert(0 <= work_index < after_work.len());
    SchedulerReservation::clone_fields(
        &before_reservations[reservation_index],
        &after_reservations[reservation_index],
    );
    WorkRecord::reservation_lifecycle_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    reveal(reservation_matches_work_phase);
    assert(reservation_matches_work_phase(
        after_work[work_index],
        after_reservations[reservation_index],
    ));
    reveal(reservation_has_matching_work);
}

proof fn cloned_active_work_has_reservation(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_reservations: Seq<SchedulerReservation>,
    work_index: int,
)
    requires
        before_work.len() == after_work.len(),
        forall |index: int| #![auto]
            0 <= index < before_work.len() ==>
                WorkRecord::reservation_lifecycle_equivalent(
                    &before_work[index],
                    &after_work[index],
                ),
        before_reservations.len() == after_reservations.len(),
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() ==>
                SchedulerReservation::clone_equivalent(
                    &before_reservations[index],
                    &after_reservations[index],
                ),
        active_work_has_reservations(before_work, before_reservations),
        0 <= work_index < after_work.len(),
    ensures active_work_has_reservation(after_work[work_index], after_reservations),
{
    assert(0 <= work_index < before_work.len());
    WorkRecord::reservation_lifecycle_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    reveal(active_work_has_reservation);
    if super::super::work_phase_retains_reservation(after_work[work_index].spec_phase()) {
        assert(super::super::work_phase_retains_reservation(
            before_work[work_index].spec_phase(),
        ));
        reveal(active_work_has_reservations);
        let reservation_index = choose |candidate: int|
            #![trigger before_reservations[candidate]]
            0 <= candidate < before_reservations.len()
                && before_reservations[candidate].spec_work_id()
                    == before_work[work_index].spec_definition().spec_id();
        assert(0 <= reservation_index < after_reservations.len());
        SchedulerReservation::clone_fields(
            &before_reservations[reservation_index],
            &after_reservations[reservation_index],
        );
    }
}

/// Exact semantic sequence clones preserve both lifecycle directions.
pub proof fn equivalent_sequences_preserve_phase(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_reservations: Seq<SchedulerReservation>,
)
    requires
        before_work.len() == after_work.len(),
        forall |index: int| #![auto]
            0 <= index < before_work.len() ==>
                WorkRecord::reservation_lifecycle_equivalent(
                    &before_work[index],
                    &after_work[index],
                ),
        before_reservations.len() == after_reservations.len(),
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() ==>
                SchedulerReservation::clone_equivalent(
                    &before_reservations[index],
                    &after_reservations[index],
                ),
        reservations_match_work_phases(before_work, before_reservations),
        active_work_has_reservations(before_work, before_reservations),
    ensures
        reservations_match_work_phases(after_work, after_reservations),
        active_work_has_reservations(after_work, after_reservations),
{
    reveal(reservations_match_work_phases);
    assert forall |reservation_index: int|
        #![trigger after_reservations[reservation_index]]
        0 <= reservation_index < after_reservations.len()
        implies reservation_has_matching_work(
            after_work,
            after_reservations[reservation_index],
        ) by {
        cloned_reservation_has_matching_work(
            before_work,
            after_work,
            before_reservations,
            after_reservations,
            reservation_index,
        );
    }
    reservations_match_work_phases_intro(after_work, after_reservations);
    reveal(active_work_has_reservations);
    assert forall |work_index: int| #![trigger after_work[work_index]]
        0 <= work_index < after_work.len()
        implies active_work_has_reservation(
            after_work[work_index],
            after_reservations,
        ) by {
        cloned_active_work_has_reservation(
            before_work,
            after_work,
            before_reservations,
            after_reservations,
            work_index,
        );
    }
    active_work_has_reservations_intro(after_work, after_reservations);
}

} // verus!
