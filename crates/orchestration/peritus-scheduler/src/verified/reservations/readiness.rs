//! Reducer admission facts connecting lifecycle and durable dispatch history.

use crate::SchedulerState;
#[cfg(verus_only)]
use crate::{DispatchId, SchedulerReservation, WorkPhase, WorkRecord};
use vstd::prelude::*;

mod clone;
mod insertion;
mod phase;
mod release;
mod start;
mod update;

#[cfg(verus_only)]
pub(crate) use clone::equivalent_sequences_preserve_phase;
#[cfg(verus_only)]
pub(crate) use insertion::inactive_work_insertion_preserves_phase;
#[cfg(verus_only)]
pub(crate) use phase::{
    active_work_has_reservation, active_work_has_reservations, active_work_has_reservations_intro,
    reservation_has_matching_work, reservation_matches_work_phase, reservations_match_work_phases,
    reservations_match_work_phases_intro, selected_insertion_establishes_phase,
};
#[cfg(verus_only)]
pub(crate) use release::release_preserves_relations;
#[cfg(verus_only)]
pub(crate) use start::start_acknowledgement_preserves_relations;
#[cfg(verus_only)]
pub(crate) use update::{
    admissible_work_update_preserves_relations, inactive_work_update_is_admissible,
    work_phase_update_admissible,
};

verus! {

/// Returns whether a work lifecycle can retain live dispatch ownership.
pub open spec fn work_phase_retains_reservation(phase: WorkPhase) -> bool {
    phase == WorkPhase::Reserved
        || phase == WorkPhase::Running
        || phase == WorkPhase::Cancelling
}

/// Returns whether one live reservation binds retained ownership-active work.
pub open spec fn reservation_has_active_work(
    work: Seq<WorkRecord>,
    reservation: SchedulerReservation,
) -> bool {
    exists |work_index: int|
        #![trigger work[work_index].spec_definition().spec_id()]
        0 <= work_index < work.len()
            && work[work_index].spec_definition().spec_id() == reservation.spec_work_id()
            && work_phase_retains_reservation(work[work_index].spec_phase())
}

/// Every live reservation binds work in an ownership-retaining lifecycle.
pub open spec fn reservations_bind_active_work(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
) -> bool {
    forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
            ==> reservation_has_active_work(work, reservations[reservation_index])
}

/// Every live reservation identity remains in the durable dispatch history.
pub open spec fn reservations_are_retained_dispatches(
    reservations: Seq<SchedulerReservation>,
    used_dispatches: Seq<DispatchId>,
) -> bool {
    forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
            ==> exists |used_index: int| #![trigger used_dispatches[used_index]]
                0 <= used_index < used_dispatches.len()
                    && used_dispatches[used_index]
                        == reservations[reservation_index].spec_dispatch_id()
}

impl SchedulerState {
    /// Returns the reducer state needed to safely admit another reservation.
    pub open spec fn spec_reservation_reducer_ready(&self) -> bool {
        &&& self.spec_reservation_invariant()
        &&& reservations_bind_active_work(self.spec_work(), self.spec_reservations())
        &&& reservations_match_work_phases(self.spec_work(), self.spec_reservations())
        &&& active_work_has_reservations(self.spec_work(), self.spec_reservations())
        &&& reservations_are_retained_dispatches(
            self.spec_reservations(),
            self.spec_used_dispatches(),
        )
    }
}

/// A dispatch absent from retained history cannot name a live reservation.
pub proof fn unused_dispatch_is_not_live(state: &SchedulerState, dispatch_id: DispatchId)
    requires
        state.spec_reservation_reducer_ready(),
        !state.spec_used_dispatches().contains(dispatch_id),
    ensures
        forall |reservation_index: int| #![auto]
            0 <= reservation_index < state.spec_reservations().len() ==>
                state.spec_reservations()[reservation_index].spec_dispatch_id() != dispatch_id,
{
    reveal(reservations_are_retained_dispatches);
    assert forall |reservation_index: int| #![auto]
        0 <= reservation_index < state.spec_reservations().len()
        implies state.spec_reservations()[reservation_index].spec_dispatch_id() != dispatch_id by {
        if state.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id {
            let used_index = choose |used_index: int|
                #![trigger state.spec_used_dispatches()[used_index]]
                0 <= used_index < state.spec_used_dispatches().len()
                    && state.spec_used_dispatches()[used_index]
                        == state.spec_reservations()[reservation_index].spec_dispatch_id();
            assert(state.spec_used_dispatches()[used_index] == dispatch_id);
            assert(state.spec_used_dispatches().contains(dispatch_id));
            assert(false);
        }
    }
}

/// Adding one admitted reservation and its durable identity establishes reducer relations.
pub proof fn selected_insertion_establishes_relations(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_reservations: Seq<SchedulerReservation>,
    before_used: Seq<DispatchId>,
    after_used: Seq<DispatchId>,
    reservation: SchedulerReservation,
    work_index: int,
    reservation_index: int,
    used_index: int,
)
    requires
        reservations_bind_active_work(before_work, before_reservations),
        reservations_are_retained_dispatches(before_reservations, before_used),
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
        0 <= used_index <= before_used.len(),
        after_used == before_used.insert(used_index, reservation.spec_dispatch_id()),
    ensures
        reservations_bind_active_work(after_work, after_reservations),
        reservations_are_retained_dispatches(after_reservations, after_used),
{
    WorkRecord::reservation_subject_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    before_reservations.insert_ensures(reservation_index, reservation);
    before_used.insert_ensures(used_index, reservation.spec_dispatch_id());
    reveal(work_phase_retains_reservation);
    reveal(reservations_bind_active_work);
    assert forall |after_index: int| #![trigger after_reservations[after_index]]
        0 <= after_index < after_reservations.len()
        implies exists |selected_work: int|
            #![trigger after_work[selected_work].spec_definition().spec_id()]
            0 <= selected_work < after_work.len()
                && after_work[selected_work].spec_definition().spec_id()
                    == after_reservations[after_index].spec_work_id()
                && work_phase_retains_reservation(after_work[selected_work].spec_phase()) by {
        if after_index == reservation_index {
            assert(after_reservations[after_index] == reservation);
            assert(exists |selected_work: int|
                #![trigger after_work[selected_work].spec_definition().spec_id()]
                0 <= selected_work < after_work.len()
                    && after_work[selected_work].spec_definition().spec_id()
                        == after_reservations[after_index].spec_work_id()
                    && work_phase_retains_reservation(
                        after_work[selected_work].spec_phase(),
                    )) by {
                assert(0 <= work_index < after_work.len());
            }
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
                    && before_work[candidate].spec_definition().spec_id()
                        == before_reservations[old_index].spec_work_id()
                    && work_phase_retains_reservation(before_work[candidate].spec_phase());
            assert(old_work != work_index) by {
                if old_work == work_index {
                    assert(before_reservations[old_index].spec_work_id()
                        == before_work[work_index].spec_definition().spec_id());
                    assert(false);
                }
            }
            assert(before_work[old_work] == after_work[old_work]);
            assert(exists |selected_work: int|
                #![trigger after_work[selected_work].spec_definition().spec_id()]
                0 <= selected_work < after_work.len()
                    && after_work[selected_work].spec_definition().spec_id()
                        == after_reservations[after_index].spec_work_id()
                    && work_phase_retains_reservation(
                        after_work[selected_work].spec_phase(),
                    )) by {
                assert(0 <= old_work < after_work.len());
            }
        }
    }
    reveal(reservations_are_retained_dispatches);
    assert forall |after_index: int| #![trigger after_reservations[after_index]]
        0 <= after_index < after_reservations.len()
        implies exists |retained_index: int| #![trigger after_used[retained_index]]
            0 <= retained_index < after_used.len()
                && after_used[retained_index]
                    == after_reservations[after_index].spec_dispatch_id() by {
        if after_index == reservation_index {
            assert(after_reservations[after_index] == reservation);
            assert(after_used[used_index] == reservation.spec_dispatch_id());
        } else {
            let old_index = if after_index < reservation_index {
                after_index
            } else {
                after_index - 1
            };
            assert(0 <= old_index < before_reservations.len());
            assert(after_reservations[after_index] == before_reservations[old_index]);
            let old_used = choose |candidate: int| #![trigger before_used[candidate]]
                0 <= candidate < before_used.len()
                    && before_used[candidate]
                        == before_reservations[old_index].spec_dispatch_id();
            let retained_index = if old_used < used_index { old_used } else { old_used + 1 };
            assert(0 <= retained_index < after_used.len());
            assert(after_used[retained_index] == before_used[old_used]);
            assert(exists |candidate: int| #![trigger after_used[candidate]]
                0 <= candidate < after_used.len()
                    && after_used[candidate]
                        == after_reservations[after_index].spec_dispatch_id()) by {
            }
        }
    }
}

/// A work bookkeeping update that retains identity and phase preserves reducer relations.
pub proof fn lifecycle_work_update_preserves_relations(
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    used_dispatches: Seq<DispatchId>,
    work_index: int,
)
    requires
        reservations_bind_active_work(before_work, reservations),
        reservations_are_retained_dispatches(reservations, used_dispatches),
        before_work.len() == after_work.len(),
        0 <= work_index < before_work.len(),
        WorkRecord::reservation_lifecycle_equivalent(
            &before_work[work_index],
            &after_work[work_index],
        ),
        forall |index: int| #![auto]
            0 <= index < before_work.len() && index != work_index ==>
                before_work[index] == after_work[index],
    ensures
        reservations_bind_active_work(after_work, reservations),
        reservations_are_retained_dispatches(reservations, used_dispatches),
{
    WorkRecord::reservation_lifecycle_fields(
        &before_work[work_index],
        &after_work[work_index],
    );
    reveal(reservations_bind_active_work);
    assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
        implies exists |selected_work: int|
            #![trigger after_work[selected_work].spec_definition().spec_id()]
            0 <= selected_work < after_work.len()
                && after_work[selected_work].spec_definition().spec_id()
                    == reservations[reservation_index].spec_work_id()
                && work_phase_retains_reservation(after_work[selected_work].spec_phase()) by {
        let old_work = choose |candidate: int|
            #![trigger before_work[candidate].spec_definition().spec_id()]
            0 <= candidate < before_work.len()
                && before_work[candidate].spec_definition().spec_id()
                    == reservations[reservation_index].spec_work_id()
                && work_phase_retains_reservation(before_work[candidate].spec_phase());
        if old_work != work_index {
            assert(before_work[old_work] == after_work[old_work]);
        }
        assert(exists |selected_work: int|
            #![trigger after_work[selected_work].spec_definition().spec_id()]
            0 <= selected_work < after_work.len()
                && after_work[selected_work].spec_definition().spec_id()
                    == reservations[reservation_index].spec_work_id()
                && work_phase_retains_reservation(
                    after_work[selected_work].spec_phase(),
                )) by {
            assert(0 <= old_work < after_work.len());
        }
    }
}

} // verus!
