//! Combined reservation release and work lifecycle transitions.

use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkTerminal};
use peritus_types::Sha256Digest;

use super::{WorkUpdate, update_work};

#[cfg(verus_only)]
mod phase_relation;
#[cfg(verus_only)]
mod readiness;
#[cfg(verus_only)]
mod terminal_relation;
#[cfg(verus_only)]
pub(crate) use phase_relation::phase_release_matches;
#[cfg(verus_only)]
use phase_relation::phase_release_preserves_collections_order;
#[cfg(verus_only)]
use readiness::establish_released_state_ready;
#[cfg(verus_only)]
pub(crate) use terminal_relation::terminal_release_matches;
#[cfg(verus_only)]
use terminal_relation::{
    release_preserves_other_state, terminal_release_preserves_collections_order,
};

verus! {

/// Returns whether a live reservation exactly names the released dispatch and work.
pub open spec fn release_target_exists(
    state: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
) -> bool {
    exists |reservation_index: int|
        #![trigger state.spec_reservations()[reservation_index]]
        0 <= reservation_index < state.spec_reservations().len()
            && state.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
            && state.spec_reservations()[reservation_index].spec_work_id() == work_id
}


fn release_with_update(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    update: WorkUpdate,
) -> (removed: Option<SchedulerReservation>)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        release_preserves_other_state(old(state), final(state)),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        match removed {
            Some(reservation) => {
                &&& super::super::reservation_remove::exact_reservation_removal_matches(
                    old(state).spec_reservations(),
                    final(state).spec_reservations(),
                    dispatch_id,
                    Some(reservation),
                )
                &&& super::exact_work_update_matches(
                    old(state).spec_work(),
                    final(state).spec_work(),
                    work_id,
                    update,
                    true,
                )
            },
            None => true,
        },
        old(state).spec_reservation_reducer_ready()
                && !crate::verified::work_phase_retains_reservation(update.spec_phase())
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> removed.is_some() && final(state).spec_reservation_reducer_ready(),
        old(state).spec_reservation_reducer_ready()
                && !crate::verified::work_phase_retains_reservation(update.spec_phase())
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> match removed {
                Some(reservation) => {
                    &&& reservation.spec_dispatch_id() == dispatch_id
                    &&& reservation.spec_work_id() == work_id
                },
                None => false,
            },
{
    let ghost before_work = state.spec_work();
    let ghost before_reservations = state.spec_reservations();
    let ghost used_dispatches = state.spec_used_dispatches();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost target_phase = update.spec_phase();
    let ghost target_exists = release_target_exists(state, dispatch_id, work_id);
    proof {
        assert(before_work == old(state).spec_work());
        assert(before_reservations == old(state).spec_reservations());
        assert(used_dispatches == old(state).spec_used_dispatches());
        assert(was_ready == old(state).spec_reservation_reducer_ready());
        assert(target_exists == release_target_exists(old(state), dispatch_id, work_id));
    }
    let Some(reservation) = super::super::remove_reservation(state, dispatch_id) else {
            proof {
                if target_exists {
                    reveal(release_target_exists);
                    let target_index = choose |reservation_index: int|
                        #![trigger before_reservations[reservation_index]]
                        0 <= reservation_index < before_reservations.len()
                            && before_reservations[reservation_index].spec_dispatch_id()
                                == dispatch_id
                            && before_reservations[reservation_index].spec_work_id() == work_id;
                    reveal(super::super::reservation_remove::reservation_removal_matches);
                    assert(before_reservations[target_index].spec_dispatch_id() != dispatch_id);
                    assert(false);
                }
                reveal(release_preserves_other_state);
            }
            return None;
    };
    let ghost after_removal = state.spec_reservations();
    proof {
        assert(state.spec_work() == before_work);
        assert(state.spec_used_dispatches() == used_dispatches);
    }
    if !update_work(state, work_id, update) {
        proof {
            if was_ready && target_exists {
                reveal(release_target_exists);
                let target_index = choose |reservation_index: int|
                    #![trigger before_reservations[reservation_index]]
                    0 <= reservation_index < before_reservations.len()
                        && before_reservations[reservation_index].spec_dispatch_id() == dispatch_id
                        && before_reservations[reservation_index].spec_work_id() == work_id;
                reveal(crate::verified::reservations_bind_active_work);
                let target_work = choose |work_index: int|
                    #![trigger before_work[work_index].spec_definition().spec_id()]
                    0 <= work_index < before_work.len()
                        && before_work[work_index].spec_definition().spec_id()
                            == before_reservations[target_index].spec_work_id()
                        && crate::verified::work_phase_retains_reservation(
                            before_work[work_index].spec_phase(),
                        );
                reveal(super::work_update_matches);
                assert(before_work[target_work].spec_definition().spec_id() == work_id);
                assert(false);
            }
            reveal(release_preserves_other_state);
        }
        return None;
    }
    proof {
        if was_ready
            && !crate::verified::work_phase_retains_reservation(target_phase)
            && target_exists
        {
            reveal(release_target_exists);
            reveal(SchedulerState::spec_reservation_reducer_ready);
            reveal(crate::verified::reservation_invariant_parts);
            establish_released_state_ready(
                state,
                before_work,
                before_reservations,
                after_removal,
                used_dispatches,
                dispatch_id,
                work_id,
                target_phase,
                reservation,
            );
        }
        assert(state.spec_reservations() == after_removal);
        assert(super::super::reservation_remove::exact_reservation_removal_matches(
            before_reservations,
            state.spec_reservations(),
            dispatch_id,
            Some(reservation),
        ));
        assert(super::exact_work_update_matches(
            before_work,
            state.spec_work(),
            work_id,
            update,
            true,
        ));
        reveal(release_preserves_other_state);
    }
    Some(reservation)
}

pub fn release_to_phase(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    phase: WorkPhase,
) -> (removed: Option<SchedulerReservation>)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        old(state).spec_reservation_reducer_ready()
                && !crate::verified::work_phase_retains_reservation(phase)
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> removed.is_some() && final(state).spec_reservation_reducer_ready(),
        old(state).spec_reservation_reducer_ready()
                && !crate::verified::work_phase_retains_reservation(phase)
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> match removed {
                Some(reservation) => phase_release_matches(
                    old(state), final(state), dispatch_id, work_id, phase, &reservation,
                ),
                None => false,
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && !crate::verified::work_phase_retains_reservation(phase)
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> final(state).spec_collections_ordered(),
{
    let result = release_with_update(state, dispatch_id, work_id, WorkUpdate::Phase(phase));
    proof {
        reveal(WorkUpdate::spec_phase);
        if old(state).spec_reservation_reducer_ready()
            && !crate::verified::work_phase_retains_reservation(phase)
            && release_target_exists(old(state), dispatch_id, work_id)
        {
            match &result {
                Some(reservation) => {
                    reveal(super::exact_work_update_matches);
                    reveal(WorkUpdate::record_matches);
                    reveal(super::work_phase_update_matches);
                    reveal(phase_release_matches);
                    if old(state).spec_collections_ordered() {
                        phase_release_preserves_collections_order(
                            old(state), state, dispatch_id, work_id, phase, reservation,
                        );
                    }
                },
                None => assert(false),
            }
        }
    }
    result
}

pub fn release_to_retry_pending(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    cause: Sha256Digest,
) -> (removed: Option<SchedulerReservation>)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        old(state).spec_reservation_reducer_ready()
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> removed.is_some() && final(state).spec_reservation_reducer_ready(),
{
    let result = release_with_update(state, dispatch_id, work_id, WorkUpdate::RetryPending(cause));
    proof {
        reveal(WorkUpdate::spec_phase);
        reveal(crate::verified::work_phase_retains_reservation);
    }
    result
}

pub fn release_to_terminal(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
    terminal: WorkTerminal,
) -> (removed: Option<SchedulerReservation>)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        old(state).spec_reservation_reducer_ready()
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> removed.is_some() && final(state).spec_reservation_reducer_ready(),
        old(state).spec_reservation_reducer_ready()
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> match removed {
                Some(reservation) => terminal_release_matches(
                    old(state),
                    final(state),
                    dispatch_id,
                    work_id,
                    terminal,
                    &reservation,
                ),
                None => false,
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && release_target_exists(old(state), dispatch_id, work_id)
            ==> final(state).spec_collections_ordered(),
{
    let result = release_with_update(state, dispatch_id, work_id, WorkUpdate::Terminal(terminal));
    proof {
        reveal(WorkUpdate::spec_phase);
        reveal(crate::verified::work_phase_retains_reservation);
        if old(state).spec_reservation_reducer_ready()
            && release_target_exists(old(state), dispatch_id, work_id)
        {
            match &result {
                Some(reservation) => {
                    reveal(super::exact_work_update_matches);
                    reveal(WorkUpdate::record_matches);
                    reveal(super::work_terminal_update_matches);
                    reveal(terminal_release_matches);
                    if old(state).spec_collections_ordered() {
                        terminal_release_preserves_collections_order(
                            old(state),
                            state,
                            dispatch_id,
                            work_id,
                            terminal,
                            reservation,
                        );
                    }
                },
                None => assert(false),
            }
        }
    }
    result
}

} // verus!
