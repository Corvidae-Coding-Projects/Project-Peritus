//! Atomic reducer mutation for dispatch start acknowledgement.

use vstd::prelude::*;

use crate::{DispatchId, SchedulerState, WorkId, WorkPhase};
#[cfg(verus_only)]
use crate::{SchedulerReservation, WorkRecord};

use super::work_update::{WorkUpdate, update_work};

verus! {

/// Returns whether one unacknowledged live reservation exactly names a dispatch and work item.
pub open spec fn start_target_exists(
    state: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
) -> bool {
    exists |reservation_index: int|
        #![trigger state.spec_reservations()[reservation_index]]
        0 <= reservation_index < state.spec_reservations().len()
            && state.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
            && state.spec_reservations()[reservation_index].spec_work_id() == work_id
            && !state.spec_reservations()[reservation_index].spec_started()
}

proof fn establish_started_state_ready(
    state: &SchedulerState,
    before_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_start: Seq<SchedulerReservation>,
    used_dispatches: Seq<DispatchId>,
    dispatch_id: DispatchId,
    work_id: WorkId,
)
    requires
        state.spec_reservation_invariant(),
        state.spec_reservations() == after_start,
        state.spec_used_dispatches() == used_dispatches,
        crate::verified::work_identities_unique(before_work),
        crate::verified::reservation_identities_unique(before_reservations),
        crate::verified::reservations_match_work_phases(before_work, before_reservations),
        crate::verified::active_work_has_reservations(before_work, before_reservations),
        crate::verified::reservations_are_retained_dispatches(
            before_reservations,
            used_dispatches,
        ),
        exists |reservation_index: int|
            #![trigger before_reservations[reservation_index]]
            0 <= reservation_index < before_reservations.len()
                && before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
                && before_reservations[reservation_index].spec_work_id() == work_id
                && !before_reservations[reservation_index].spec_started(),
        super::reservation_update::reservation_start_update_matches(
            before_reservations,
            after_start,
            dispatch_id,
            true,
        ),
        super::work_update::work_update_matches(
            before_work,
            state.spec_work(),
            work_id,
            WorkPhase::Running,
            true,
        ),
    ensures state.spec_reservation_reducer_ready(),
{
    let target_index = choose |reservation_index: int|
        #![trigger before_reservations[reservation_index]]
        0 <= reservation_index < before_reservations.len()
            && before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
            && before_reservations[reservation_index].spec_work_id() == work_id
            && !before_reservations[reservation_index].spec_started();
    reveal(super::reservation_update::reservation_start_update_matches);
    let started_index = choose |reservation_index: int|
        #![trigger before_reservations[reservation_index]] {
        &&& 0 <= reservation_index < before_reservations.len()
        &&& before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
        &&& SchedulerReservation::invariant_equivalent(
            &before_reservations[reservation_index],
            &after_start[reservation_index],
        )
        &&& after_start[reservation_index].spec_started()
        &&& forall |other: int| #![auto]
            0 <= other < before_reservations.len() && other != reservation_index ==>
                after_start[other] == before_reservations[other]
    };
    reveal(crate::verified::reservation_identities_unique);
    assert(started_index == target_index) by {
        if started_index != target_index {
            assert(before_reservations[started_index].spec_dispatch_id()
                != before_reservations[target_index].spec_dispatch_id());
            assert(false);
        }
    }
    reveal(super::work_update::work_update_matches);
    let work_index = choose |index: int| #![trigger before_work[index]] {
        &&& 0 <= index < before_work.len()
        &&& super::work_update::work_record_update_matches(
            before_work[index],
            state.spec_work()[index],
            work_id,
            WorkPhase::Running,
        )
        &&& forall |other: int| #![auto]
            0 <= other < before_work.len() && other != index ==>
                state.spec_work()[other] == before_work[other]
    };
    reveal(super::work_update::work_record_update_matches);
    assert(before_reservations[started_index].spec_work_id()
        == before_work[work_index].spec_definition().spec_id());
    crate::verified::start_acknowledgement_preserves_relations(
        before_work,
        state.spec_work(),
        before_reservations,
        state.spec_reservations(),
        used_dispatches,
        work_index,
        started_index,
    );
    reveal(SchedulerState::spec_reservation_reducer_ready);
}

/// Marks the reservation started, then moves its work to running.
///
/// `None` retains the legacy dispatch-disappeared result. `Some(false)` retains
/// the legacy work-disappeared result after the start bit was updated.
pub fn acknowledge_reservation_start(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
) -> (result: Option<bool>)
    ensures
        old(state).spec_reservation_reducer_ready()
                && start_target_exists(old(state), dispatch_id, work_id)
            ==> result == Some(true) && final(state).spec_reservation_reducer_ready(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
{
    let ghost before_work = state.spec_work();
    let ghost before_reservations = state.spec_reservations();
    let ghost used_dispatches = state.spec_used_dispatches();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost target_exists = start_target_exists(state, dispatch_id, work_id);
    proof {
        assert(before_work == old(state).spec_work());
        assert(before_reservations == old(state).spec_reservations());
        assert(used_dispatches == old(state).spec_used_dispatches());
        assert(was_ready == old(state).spec_reservation_reducer_ready());
        assert(target_exists == start_target_exists(old(state), dispatch_id, work_id));
    }
    if !super::mark_reservation_started(state, dispatch_id) {
        proof {
            if target_exists {
                reveal(start_target_exists);
                let target_index = choose |reservation_index: int|
                    #![trigger before_reservations[reservation_index]]
                    0 <= reservation_index < before_reservations.len()
                        && before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
                        && before_reservations[reservation_index].spec_work_id() == work_id
                        && !before_reservations[reservation_index].spec_started();
                reveal(super::reservation_update::reservation_start_update_matches);
                assert(before_reservations[target_index].spec_dispatch_id() != dispatch_id);
                assert(false);
            }
        }
        return None;
    }
    let ghost after_start = state.spec_reservations();
    proof {
        assert(state.spec_work() == before_work);
        assert(state.spec_used_dispatches() == used_dispatches);
    }
    if !update_work(state, work_id, WorkUpdate::Phase(WorkPhase::Running)) {
        proof {
            if was_ready && target_exists {
                reveal(start_target_exists);
                let target_index = choose |reservation_index: int|
                    #![trigger before_reservations[reservation_index]]
                    0 <= reservation_index < before_reservations.len()
                        && before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
                        && before_reservations[reservation_index].spec_work_id() == work_id
                        && !before_reservations[reservation_index].spec_started();
                reveal(crate::verified::reservations_bind_active_work);
                let target_work = choose |work_index: int|
                    #![trigger before_work[work_index].spec_definition().spec_id()]
                    0 <= work_index < before_work.len()
                        && before_work[work_index].spec_definition().spec_id()
                            == before_reservations[target_index].spec_work_id()
                        && crate::verified::work_phase_retains_reservation(
                            before_work[work_index].spec_phase(),
                        );
                reveal(super::work_update::work_update_matches);
                reveal(WorkUpdate::spec_phase);
                assert(before_work[target_work].spec_definition().spec_id() == work_id);
                assert(false);
            }
        }
        return Some(false);
    }
    proof {
        if was_ready && target_exists {
            reveal(start_target_exists);
            reveal(SchedulerState::spec_reservation_reducer_ready);
            reveal(crate::verified::reservation_invariant_parts);
            reveal(WorkUpdate::spec_phase);
            establish_started_state_ready(
                state,
                before_work,
                before_reservations,
                after_start,
                used_dispatches,
                dispatch_id,
                work_id,
            );
        }
    }
    Some(true)
}

} // verus!
