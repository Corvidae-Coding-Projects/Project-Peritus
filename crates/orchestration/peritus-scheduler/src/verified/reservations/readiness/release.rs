//! Lifecycle correspondence after exact reservation release and work update.

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

/// Removing one reservation and moving its unique work item to an inactive phase
/// preserves the reducer's bidirectional lifecycle and retained-history relations.
pub proof fn release_preserves_relations(
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
        after_reservations == before_reservations.remove(reservation_index),
        after_work.len() == before_work.len(),
        WorkRecord::reservation_binding_equivalent(
            &before_work[work_index],
            &after_work[work_index],
        ),
        !work_phase_retains_reservation(after_work[work_index].spec_phase()),
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
    before_reservations.remove_ensures(reservation_index);
    reveal(reservation_identities_unique);
    reveal(work_identities_unique);
    reveal(reservations_match_work_phases);
    assert forall |after_index: int| #![trigger after_reservations[after_index]]
        0 <= after_index < after_reservations.len()
        implies reservation_has_matching_work(
            after_work,
            after_reservations[after_index],
        ) by {
        let old_index = if after_index < reservation_index {
            after_index
        } else {
            after_index + 1
        };
        assert(0 <= old_index < before_reservations.len());
        assert(old_index != reservation_index);
        assert(after_reservations[after_index] == before_reservations[old_index]);
        let old_work = choose |candidate: int|
            #![trigger before_work[candidate].spec_definition().spec_id()]
            0 <= candidate < before_work.len()
                && reservation_matches_work_phase(
                    before_work[candidate],
                    before_reservations[old_index],
                );
        reveal(reservation_matches_work_phase);
        assert(old_work != work_index) by {
            if old_work == work_index {
                assert(before_reservations[old_index].spec_work_id()
                    == before_work[work_index].spec_definition().spec_id());
                assert(before_reservations[old_index].spec_work_id()
                    == before_reservations[reservation_index].spec_work_id());
                assert(false);
            }
        }
        assert(after_work[old_work] == before_work[old_work]);
        reveal(reservation_has_matching_work);
        assert(reservation_matches_work_phase(
            after_work[old_work],
            after_reservations[after_index],
        ));
    }
    reservations_match_work_phases_intro(after_work, after_reservations);

    reveal(active_work_has_reservations);
    assert forall |after_index: int| #![trigger after_work[after_index]]
        0 <= after_index < after_work.len()
        implies active_work_has_reservation(after_work[after_index], after_reservations) by {
        reveal(active_work_has_reservation);
        if work_phase_retains_reservation(after_work[after_index].spec_phase()) {
            assert(after_index != work_index);
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
            let retained_index = if old_reservation < reservation_index {
                old_reservation
            } else {
                old_reservation - 1
            };
            assert(0 <= retained_index < after_reservations.len());
            assert(after_reservations[retained_index]
                == before_reservations[old_reservation]);
        }
    }
    active_work_has_reservations_intro(after_work, after_reservations);

    reveal(reservations_bind_active_work);
    assert forall |reservation_index: int| #![trigger after_reservations[reservation_index]]
        0 <= reservation_index < after_reservations.len()
        implies exists |selected_work: int|
            #![trigger after_work[selected_work].spec_definition().spec_id()]
            0 <= selected_work < after_work.len()
                && after_work[selected_work].spec_definition().spec_id()
                    == after_reservations[reservation_index].spec_work_id()
                && work_phase_retains_reservation(after_work[selected_work].spec_phase()) by {
        let selected_work = choose |candidate: int|
            #![trigger after_work[candidate].spec_definition().spec_id()]
            0 <= candidate < after_work.len()
                && reservation_matches_work_phase(
                    after_work[candidate],
                    after_reservations[reservation_index],
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
        let old_index = if after_index < reservation_index {
            after_index
        } else {
            after_index + 1
        };
        assert(0 <= old_index < before_reservations.len());
        assert(after_reservations[after_index] == before_reservations[old_index]);
        let retained_index = choose |candidate: int| #![trigger used_dispatches[candidate]]
            0 <= candidate < used_dispatches.len()
                && used_dispatches[candidate]
                    == before_reservations[old_index].spec_dispatch_id();
        assert(0 <= retained_index < used_dispatches.len());
    }
}

} // verus!
