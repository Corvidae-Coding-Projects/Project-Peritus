//! Bidirectional lifecycle compatibility between work and live reservations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{SchedulerReservation, WorkPhase, WorkRecord};

verus! {

/// Returns whether one reservation agrees with its retained work lifecycle and start bit.
pub open spec fn reservation_matches_work_phase(
    work: WorkRecord,
    reservation: SchedulerReservation,
) -> bool {
    &&& work.spec_definition().spec_id() == reservation.spec_work_id()
    &&& (work.spec_phase() == WorkPhase::Reserved && !reservation.spec_started()
        || work.spec_phase() == WorkPhase::Running && reservation.spec_started()
        || work.spec_phase() == WorkPhase::Cancelling)
}

/// Every reservation has retained work whose lifecycle agrees with acknowledgement state.
pub open spec fn reservation_has_matching_work(
    work: Seq<WorkRecord>,
    reservation: SchedulerReservation,
) -> bool {
    exists |work_index: int|
        #![trigger work[work_index].spec_definition().spec_id()]
        0 <= work_index < work.len()
            && reservation_matches_work_phase(work[work_index], reservation)
}

/// Returns whether active work has a matching live reservation.
pub open spec fn active_work_has_reservation(
    work: WorkRecord,
    reservations: Seq<SchedulerReservation>,
) -> bool {
    super::work_phase_retains_reservation(work.spec_phase()) ==>
        exists |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len()
                && reservations[reservation_index].spec_work_id()
                    == work.spec_definition().spec_id()
}

/// Every reservation has retained work whose lifecycle agrees with acknowledgement state.
pub open spec fn reservations_match_work_phases(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
) -> bool {
    forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len() ==>
            reservation_has_matching_work(work, reservations[reservation_index])
}

/// Every work lifecycle that retains ownership has one live reservation.
pub open spec fn active_work_has_reservations(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
) -> bool {
    forall |work_index: int| #![trigger work[work_index]]
        0 <= work_index < work.len() ==>
            active_work_has_reservation(work[work_index], reservations)
}

/// Folds the exact per-reservation lifecycle witness into the aggregate relation.
pub proof fn reservations_match_work_phases_intro(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
)
    requires forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len() ==>
            reservation_has_matching_work(work, reservations[reservation_index]),
    ensures reservations_match_work_phases(work, reservations),
{
    reveal(reservations_match_work_phases);
}

/// Folds the exact per-work ownership witness into the aggregate relation.
pub proof fn active_work_has_reservations_intro(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
)
    requires forall |work_index: int| #![trigger work[work_index]]
        0 <= work_index < work.len() ==>
            active_work_has_reservation(work[work_index], reservations),
    ensures active_work_has_reservations(work, reservations),
{
    reveal(active_work_has_reservations);
}

/// Adding the selected unacknowledged reservation establishes both lifecycle directions.
pub proof fn selected_insertion_establishes_phase(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_reservations: Seq<SchedulerReservation>,
    reservation: SchedulerReservation,
    work_index: int,
    reservation_index: int,
)
    requires
        reservations_match_work_phases(before_work, before_reservations),
        active_work_has_reservations(before_work, before_reservations),
        0 <= work_index < before_work.len(),
        before_work.len() == after_work.len(),
        before_work[work_index].spec_phase() == WorkPhase::Queued,
        after_work[work_index].spec_phase() == WorkPhase::Reserved,
        WorkRecord::reservation_subject_equivalent(
            &before_work[work_index],
            &after_work[work_index],
        ),
        forall |index: int| #![auto]
            0 <= index < before_work.len() && index != work_index ==>
                before_work[index] == after_work[index],
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() ==>
                before_reservations[index].spec_work_id()
                    != before_work[work_index].spec_definition().spec_id(),
        0 <= reservation_index <= before_reservations.len(),
        after_reservations
            == before_reservations.insert(reservation_index, reservation),
        reservation.spec_work_id()
            == after_work[work_index].spec_definition().spec_id(),
        !reservation.spec_started(),
    ensures
        reservations_match_work_phases(after_work, after_reservations),
        active_work_has_reservations(after_work, after_reservations),
{
    WorkRecord::reservation_subject_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    before_reservations.insert_ensures(reservation_index, reservation);
    reveal(super::work_phase_retains_reservation);
    reveal(reservation_matches_work_phase);
    reveal(reservations_match_work_phases);
    assert forall |after_index: int| #![trigger after_reservations[after_index]]
        0 <= after_index < after_reservations.len()
        implies reservation_has_matching_work(
            after_work,
            after_reservations[after_index],
        ) by {
        reveal(reservation_has_matching_work);
        if after_index == reservation_index {
            assert(after_reservations[after_index] == reservation);
            assert(reservation_matches_work_phase(
                after_work[work_index],
                after_reservations[after_index],
            ));
        } else {
            let old_index = if after_index < reservation_index {
                after_index
            } else {
                after_index - 1
            };
            assert(0 <= old_index < before_reservations.len());
            assert(after_reservations[after_index] == before_reservations[old_index]);
            let old_work = choose |candidate: int|
                #![trigger before_work[candidate].spec_definition().spec_id()]
                0 <= candidate < before_work.len()
                    && reservation_matches_work_phase(
                        before_work[candidate],
                        before_reservations[old_index],
                    );
            assert(old_work != work_index) by {
                if old_work == work_index {
                    assert(before_reservations[old_index].spec_work_id()
                        == before_work[work_index].spec_definition().spec_id());
                    assert(false);
                }
            }
            assert(before_work[old_work] == after_work[old_work]);
            assert(reservation_matches_work_phase(
                after_work[old_work],
                after_reservations[after_index],
            ));
        }
    }
    reservations_match_work_phases_intro(after_work, after_reservations);
    reveal(active_work_has_reservations);
    assert forall |after_index: int| #![trigger after_work[after_index]]
        0 <= after_index < after_work.len()
        implies active_work_has_reservation(
            after_work[after_index],
            after_reservations,
        ) by {
        reveal(active_work_has_reservation);
        if super::work_phase_retains_reservation(after_work[after_index].spec_phase()) {
            if after_index == work_index {
                assert(after_reservations[reservation_index] == reservation);
            } else {
                assert(before_work[after_index] == after_work[after_index]);
                let old_reservation = choose |candidate: int|
                    #![trigger before_reservations[candidate]]
                    0 <= candidate < before_reservations.len()
                        && before_reservations[candidate].spec_work_id()
                            == before_work[after_index].spec_definition().spec_id();
                let selected_reservation = if old_reservation < reservation_index {
                    old_reservation
                } else {
                    old_reservation + 1
                };
                assert(0 <= selected_reservation < after_reservations.len());
                assert(after_reservations[selected_reservation]
                    == before_reservations[old_reservation]);
            }
        }
    }
    active_work_has_reservations_intro(after_work, after_reservations);
}

} // verus!
