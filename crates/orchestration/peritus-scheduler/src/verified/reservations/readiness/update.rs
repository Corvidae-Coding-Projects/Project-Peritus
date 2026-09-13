//! Readiness preservation for compatible standalone work phase updates.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkRecord};

#[cfg(verus_only)]
use super::{
    active_work_has_reservation, active_work_has_reservations, active_work_has_reservations_intro,
    reservation_has_matching_work, reservation_matches_work_phase,
    reservations_are_retained_dispatches, reservations_bind_active_work,
    reservations_match_work_phases, reservations_match_work_phases_intro,
    work_phase_retains_reservation,
};

#[cfg(verus_only)]
use crate::verified::work_identities_unique;

verus! {

/// Returns whether a standalone phase update agrees with current reservation ownership.
pub open spec fn work_phase_update_admissible(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
    target: WorkPhase,
) -> bool {
    &&& exists |work_index: int| #![trigger work[work_index]]
        0 <= work_index < work.len()
            && work[work_index].spec_definition().spec_id() == id
    &&& if target == WorkPhase::Cancelling {
        exists |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len()
                && reservations[reservation_index].spec_work_id() == id
    } else if !work_phase_retains_reservation(target) {
        forall |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len() ==>
                reservations[reservation_index].spec_work_id() != id
    } else {
        false
    }
}

/// An inactive retained work item can move to any other inactive phase without live ownership.
pub proof fn inactive_work_update_is_admissible(
    state: &SchedulerState,
    work_index: int,
    target: WorkPhase,
)
    requires
        state.spec_reservation_reducer_ready(),
        0 <= work_index < state.spec_work().len(),
        !work_phase_retains_reservation(state.spec_work()[work_index].spec_phase()),
        !work_phase_retains_reservation(target),
    ensures work_phase_update_admissible(
        state.spec_work(),
        state.spec_reservations(),
        state.spec_work()[work_index].spec_definition().spec_id(),
        target,
    ),
{
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(reservations_match_work_phases);
    reveal(reservation_has_matching_work);
    reveal(reservation_matches_work_phase);
    reveal(work_identities_unique);
    let id = state.spec_work()[work_index].spec_definition().spec_id();
    assert forall |reservation_index: int| #![trigger state.spec_reservations()[reservation_index]]
        0 <= reservation_index < state.spec_reservations().len() implies
            state.spec_reservations()[reservation_index].spec_work_id() != id by {
        if state.spec_reservations()[reservation_index].spec_work_id() == id {
            let matching_work = choose |candidate: int|
                #![trigger state.spec_work()[candidate].spec_definition().spec_id()]
                0 <= candidate < state.spec_work().len()
                    && reservation_matches_work_phase(
                        state.spec_work()[candidate],
                        state.spec_reservations()[reservation_index],
                    );
            assert(state.spec_work()[matching_work].spec_definition().spec_id() == id);
            assert(matching_work == work_index) by {
                if matching_work != work_index {
                    assert(state.spec_work()[matching_work].spec_definition().spec_id()
                        != state.spec_work()[work_index].spec_definition().spec_id());
                    assert(false);
                }
            }
            assert(work_phase_retains_reservation(
                state.spec_work()[work_index].spec_phase(),
            ));
            assert(false);
        }
    }
    reveal(work_phase_update_admissible);
}

proof fn admissible_work_update_preserves_active_binding(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
    target: WorkPhase,
    work_index: int,
)
    requires
        reservations_bind_active_work(before_work, reservations),
        work_phase_update_admissible(before_work, reservations, id, target),
        before_work.len() == after_work.len(),
        0 <= work_index < before_work.len(),
        before_work[work_index].spec_definition().spec_id() == id,
        WorkRecord::reservation_binding_equivalent(
            &before_work[work_index],
            &after_work[work_index],
        ),
        after_work[work_index].spec_phase() == target,
        forall |index: int| #![auto]
            0 <= index < before_work.len() && index != work_index ==>
                after_work[index] == before_work[index],
    ensures reservations_bind_active_work(after_work, reservations),
{
    WorkRecord::reservation_binding_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    reveal(work_phase_update_admissible);
    reveal(reservations_bind_active_work);
    assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
        implies super::reservation_has_active_work(
            after_work,
            reservations[reservation_index],
        ) by {
        assert(super::reservation_has_active_work(
            before_work,
            reservations[reservation_index],
        ));
        reveal(super::reservation_has_active_work);
        let old_work = choose |candidate: int|
            #![trigger before_work[candidate].spec_definition().spec_id()]
            0 <= candidate < before_work.len()
                && before_work[candidate].spec_definition().spec_id()
                    == reservations[reservation_index].spec_work_id()
                && work_phase_retains_reservation(before_work[candidate].spec_phase());
        if old_work == work_index {
            assert(reservations[reservation_index].spec_work_id() == id);
            if target != WorkPhase::Cancelling {
                assert(!work_phase_retains_reservation(target));
                assert(reservations[reservation_index].spec_work_id() != id);
                assert(false);
            }
        } else {
            assert(after_work[old_work] == before_work[old_work]);
        }
        assert(0 <= old_work < after_work.len());
        assert(after_work[old_work].spec_definition().spec_id()
            == reservations[reservation_index].spec_work_id());
        assert(work_phase_retains_reservation(after_work[old_work].spec_phase()));
        assert(super::reservation_has_active_work(
            after_work,
            reservations[reservation_index],
        ));
    }
}

/// A compatible exact work update preserves every reducer readiness relation.
pub proof fn admissible_work_update_preserves_relations(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    used_dispatches: Seq<DispatchId>,
    id: WorkId,
    target: WorkPhase,
    work_index: int,
)
    requires
        work_identities_unique(before_work),
        reservations_bind_active_work(before_work, reservations),
        reservations_match_work_phases(before_work, reservations),
        active_work_has_reservations(before_work, reservations),
        reservations_are_retained_dispatches(reservations, used_dispatches),
        work_phase_update_admissible(before_work, reservations, id, target),
        before_work.len() == after_work.len(),
        0 <= work_index < before_work.len(),
        before_work[work_index].spec_definition().spec_id() == id,
        WorkRecord::reservation_binding_equivalent(
            &before_work[work_index],
            &after_work[work_index],
        ),
        after_work[work_index].spec_phase() == target,
        forall |index: int| #![auto]
            0 <= index < before_work.len() && index != work_index ==>
                after_work[index] == before_work[index],
    ensures
        reservations_match_work_phases(after_work, reservations),
        active_work_has_reservations(after_work, reservations),
        reservations_bind_active_work(after_work, reservations),
        reservations_are_retained_dispatches(reservations, used_dispatches),
{
    WorkRecord::reservation_binding_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    reveal(work_identities_unique);
    reveal(work_phase_update_admissible);
    let admitted_work = choose |candidate: int| #![trigger before_work[candidate]]
        0 <= candidate < before_work.len()
            && before_work[candidate].spec_definition().spec_id() == id;
    assert(admitted_work == work_index) by {
        if admitted_work != work_index {
            assert(before_work[admitted_work].spec_definition().spec_id()
                != before_work[work_index].spec_definition().spec_id());
            assert(false);
        }
    }

    reveal(reservations_match_work_phases);
    assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
        implies reservation_has_matching_work(after_work, reservations[reservation_index]) by {
        reveal(reservation_has_matching_work);
        let old_work = choose |candidate: int|
            #![trigger before_work[candidate].spec_definition().spec_id()]
            0 <= candidate < before_work.len()
                && reservation_matches_work_phase(
                    before_work[candidate],
                    reservations[reservation_index],
                );
        reveal(reservation_matches_work_phase);
        if old_work == work_index {
            assert(reservations[reservation_index].spec_work_id() == id);
            if target != WorkPhase::Cancelling {
                assert(!work_phase_retains_reservation(target));
                assert(reservations[reservation_index].spec_work_id() != id);
                assert(false);
            }
            assert(reservation_matches_work_phase(
                after_work[work_index],
                reservations[reservation_index],
            ));
        } else {
            assert(after_work[old_work] == before_work[old_work]);
            assert(reservation_matches_work_phase(
                after_work[old_work],
                reservations[reservation_index],
            ));
        }
    }
    reservations_match_work_phases_intro(after_work, reservations);

    reveal(active_work_has_reservations);
    assert forall |after_index: int| #![trigger after_work[after_index]]
        0 <= after_index < after_work.len()
        implies active_work_has_reservation(after_work[after_index], reservations) by {
        reveal(active_work_has_reservation);
        if work_phase_retains_reservation(after_work[after_index].spec_phase()) {
            if after_index == work_index {
                assert(target == WorkPhase::Cancelling);
                let reservation_index = choose |candidate: int|
                    #![trigger reservations[candidate]]
                    0 <= candidate < reservations.len()
                        && reservations[candidate].spec_work_id() == id;
                assert(reservations[reservation_index].spec_work_id()
                    == after_work[after_index].spec_definition().spec_id());
            } else {
                assert(after_work[after_index] == before_work[after_index]);
            }
        }
    }
    active_work_has_reservations_intro(after_work, reservations);

    admissible_work_update_preserves_active_binding(
        before_work,
        after_work,
        reservations,
        id,
        target,
        work_index,
    );
}

} // verus!
