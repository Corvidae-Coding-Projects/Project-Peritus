//! Lifecycle correspondence after start acknowledgement.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{DispatchId, SchedulerReservation, WorkPhase, WorkRecord};

#[cfg(verus_only)]
use super::{
    active_work_has_reservation, active_work_has_reservations, active_work_has_reservations_intro,
    reservation_has_matching_work, reservation_matches_work_phase,
    reservations_are_retained_dispatches, reservations_bind_active_work,
    reservations_match_work_phases, reservations_match_work_phases_intro,
    work_phase_retains_reservation,
};

#[cfg(verus_only)]
use crate::verified::{reservation_identities_unique, work_identities_unique};

verus! {

/// Marking one unique reservation started and moving its unique work to running
/// preserves the reducer's bidirectional lifecycle and retained-history relations.
pub proof fn start_acknowledgement_preserves_relations(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_reservations: Seq<SchedulerReservation>,
    used_dispatches: Seq<DispatchId>,
    work_index: int,
    reservation_index: int,
)
    requires
        work_identities_unique(before_work),
        reservation_identities_unique(before_reservations),
        reservations_match_work_phases(before_work, before_reservations),
        active_work_has_reservations(before_work, before_reservations),
        reservations_are_retained_dispatches(before_reservations, used_dispatches),
        0 <= work_index < before_work.len(),
        0 <= reservation_index < before_reservations.len(),
        before_reservations[reservation_index].spec_work_id()
            == before_work[work_index].spec_definition().spec_id(),
        !before_reservations[reservation_index].spec_started(),
        after_reservations.len() == before_reservations.len(),
        SchedulerReservation::invariant_equivalent(
            &before_reservations[reservation_index],
            &after_reservations[reservation_index],
        ),
        after_reservations[reservation_index].spec_started(),
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() && index != reservation_index ==>
                after_reservations[index] == before_reservations[index],
        after_work.len() == before_work.len(),
        WorkRecord::reservation_binding_equivalent(
            &before_work[work_index],
            &after_work[work_index],
        ),
        after_work[work_index].spec_phase() == WorkPhase::Running,
        forall |index: int| #![auto]
            0 <= index < before_work.len() && index != work_index ==>
                after_work[index] == before_work[index],
    ensures
        reservations_match_work_phases(after_work, after_reservations),
        active_work_has_reservations(after_work, after_reservations),
        reservations_bind_active_work(after_work, after_reservations),
        reservations_are_retained_dispatches(after_reservations, used_dispatches),
{
    WorkRecord::reservation_binding_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    SchedulerReservation::invariant_fields(
        &before_reservations[reservation_index],
        &after_reservations[reservation_index],
    );
    reveal(reservation_identities_unique);
    reveal(work_identities_unique);
    reveal(reservations_match_work_phases);
    assert forall |after_index: int| #![trigger after_reservations[after_index]]
        0 <= after_index < after_reservations.len()
        implies reservation_has_matching_work(
            after_work,
            after_reservations[after_index],
        ) by {
        reveal(reservation_has_matching_work);
        reveal(reservation_matches_work_phase);
        if after_index == reservation_index {
            assert(after_reservations[after_index].spec_work_id()
                == after_work[work_index].spec_definition().spec_id());
            assert(reservation_matches_work_phase(
                after_work[work_index],
                after_reservations[after_index],
            ));
        } else {
            assert(after_reservations[after_index] == before_reservations[after_index]);
            let old_work = choose |candidate: int|
                #![trigger before_work[candidate].spec_definition().spec_id()]
                0 <= candidate < before_work.len()
                    && reservation_matches_work_phase(
                        before_work[candidate],
                        before_reservations[after_index],
                    );
            assert(old_work != work_index) by {
                if old_work == work_index {
                    assert(before_reservations[after_index].spec_work_id()
                        == before_reservations[reservation_index].spec_work_id());
                    assert(false);
                }
            }
            assert(after_work[old_work] == before_work[old_work]);
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
        implies active_work_has_reservation(after_work[after_index], after_reservations) by {
        reveal(active_work_has_reservation);
        if work_phase_retains_reservation(after_work[after_index].spec_phase()) {
            if after_index == work_index {
                assert(after_reservations[reservation_index].spec_work_id()
                    == after_work[after_index].spec_definition().spec_id());
            } else {
                assert(after_work[after_index] == before_work[after_index]);
                let old_reservation = choose |candidate: int|
                    #![trigger before_reservations[candidate]]
                    0 <= candidate < before_reservations.len()
                        && before_reservations[candidate].spec_work_id()
                            == before_work[after_index].spec_definition().spec_id();
                assert(old_reservation != reservation_index) by {
                    if old_reservation == reservation_index {
                        assert(before_work[after_index].spec_definition().spec_id()
                            == before_work[work_index].spec_definition().spec_id());
                        assert(false);
                    }
                }
                assert(after_reservations[old_reservation]
                    == before_reservations[old_reservation]);
            }
        }
    }
    active_work_has_reservations_intro(after_work, after_reservations);

    reveal(reservations_bind_active_work);
    assert forall |current_index: int| #![trigger after_reservations[current_index]]
        0 <= current_index < after_reservations.len()
        implies exists |selected_work: int|
            #![trigger after_work[selected_work].spec_definition().spec_id()]
            0 <= selected_work < after_work.len()
                && after_work[selected_work].spec_definition().spec_id()
                    == after_reservations[current_index].spec_work_id()
                && work_phase_retains_reservation(after_work[selected_work].spec_phase()) by {
        let selected_work = choose |candidate: int|
            #![trigger after_work[candidate].spec_definition().spec_id()]
            0 <= candidate < after_work.len()
                && reservation_matches_work_phase(
                    after_work[candidate],
                    after_reservations[current_index],
                );
        reveal(reservation_matches_work_phase);
        reveal(work_phase_retains_reservation);
        assert(work_phase_retains_reservation(after_work[selected_work].spec_phase()));
    }

    reveal(reservations_are_retained_dispatches);
    assert forall |after_index: int| #![trigger after_reservations[after_index]]
        0 <= after_index < after_reservations.len()
        implies exists |retained_index: int| #![trigger used_dispatches[retained_index]]
            0 <= retained_index < used_dispatches.len()
                && used_dispatches[retained_index]
                    == after_reservations[after_index].spec_dispatch_id() by {
        let retained_index = choose |candidate: int| #![trigger used_dispatches[candidate]]
            0 <= candidate < used_dispatches.len()
                && used_dispatches[candidate]
                    == before_reservations[after_index].spec_dispatch_id();
        if after_index == reservation_index {
            assert(after_reservations[after_index].spec_dispatch_id()
                == before_reservations[after_index].spec_dispatch_id());
        } else {
            assert(after_reservations[after_index] == before_reservations[after_index]);
        }
        assert(0 <= retained_index < used_dispatches.len());
    }
}

} // verus!
