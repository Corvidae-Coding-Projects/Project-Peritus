//! Exact cancellation lifecycle updates for a preselected work sequence.

use crate::state::mutation;
#[cfg(verus_only)]
use crate::{SchedulerReservation, WorkRecord};
use crate::{SchedulerState, WorkId, WorkPhase, WorkTerminal};
use vstd::prelude::*;

mod activity;
#[cfg(verus_only)]
mod frame;
#[cfg(verus_only)]
mod trace;

use activity::is_active;

#[cfg(verus_only)]
use frame::{
    cancellation_step_preserves_ids, cancellation_step_target_exists,
    cancellation_update_admissible, collection_order_preserved, target_existence_preserved,
};
#[cfg(verus_only)]
pub(super) use trace::active_target_finishes_cancelling;
#[cfg(verus_only)]
use trace::{advance_prefix, empty_updates, establish_complete_result, establish_missing_result};

verus! {

/// Whether a retained reservation owns the supplied work identity.
pub open spec fn has_active_reservation(
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
) -> bool {
    exists |index: int| #![trigger reservations[index]]
        0 <= index < reservations.len() && reservations[index].spec_work_id() == id
}

/// Whether the supplied identity is retained by the work sequence.
pub open spec fn target_exists(work: Seq<WorkRecord>, id: WorkId) -> bool {
    exists |index: int| #![trigger work[index]]
        0 <= index < work.len() && work[index].spec_definition().spec_id() == id
}

/// Exact retained-index identity correspondence across lifecycle-only updates.
pub open spec fn work_id_layout_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
) -> bool {
    &&& before.len() == after.len()
    &&& forall |index: int| #![trigger before[index]] 0 <= index < before.len() ==>
        after[index].spec_definition().spec_id()
            == before[index].spec_definition().spec_id()
}

/// Whether every requested cancellation target exists in the original work sequence.
pub open spec fn all_targets_exist(
    work: Seq<WorkRecord>,
    affected: Seq<WorkId>,
) -> bool {
    forall |target: int| #![trigger affected[target]] 0 <= target < affected.len() ==>
        target_exists(work, affected[target])
}

/// Exact effect of one successful cancellation update.
pub open spec fn cancellation_step_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    id: WorkId,
) -> bool {
    if has_active_reservation(reservations, id) {
        mutation::work_phase_update_matches(
            before,
            after,
            id,
            WorkPhase::Cancelling,
            true,
        )
    } else {
        mutation::work_terminal_update_matches(
            before,
            after,
            id,
            WorkTerminal::Cancelled,
            true,
        )
    }
}

/// Exact ordered composition of all successful cancellation updates.
///
/// Every element in `affected` contributes one concrete changed-index step. Therefore, when
/// `affected` has no duplicates, every selected identity is updated exactly once. Each step also
/// carries the complete immutable-definition and bookkeeping preservation relation of the work
/// mutation kernel.
pub open spec fn cancellation_updates_match(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
) -> bool {
    exists |states: Seq<Seq<WorkRecord>>| {
        &&& states.len() == affected.len() + 1
        &&& states[0] == before
        &&& states[affected.len() as int] == after
        &&& forall |step: int| #![trigger states[step]] 0 <= step < affected.len() ==>
            cancellation_step_matches(
                states[step],
                states[step + 1],
                reservations,
                affected[step],
            )
    }
}

/// Exact success or first-missing-target behavior of the production update loop.
pub open spec fn cancellation_result_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
    complete: bool,
) -> bool {
    if complete {
        cancellation_updates_match(before, after, reservations, affected)
    } else {
        exists |processed: int| {
            &&& 0 <= processed < affected.len()
            &&& cancellation_updates_match(
                before, after, reservations, affected.take(processed),
            )
            &&& processed < affected.len()
            &&& !target_exists(before, affected[processed])
            &&& forall |prior: int| #![trigger affected[prior]] 0 <= prior < processed ==>
                target_exists(before, affected[prior])
        }
    }
}

/// Records whose identities were not selected remain exactly equal, not merely equivalent.
pub open spec fn unselected_work_unchanged(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    affected: Seq<WorkId>,
) -> bool {
    &&& before.len() == after.len()
    &&& forall |index: int| #![trigger before[index]] 0 <= index < before.len()
            && !affected.contains(before[index].spec_definition().spec_id()) ==>
        after[index] == before[index]
}

/// Complete final work relation for a successful preselected cancellation batch.
///
/// The ordered trace fixes every selected step to `Cancelling` with its retry and terminal fields
/// retained when a reservation exists, or to exact `Cancelled` terminal payload with retry cause
/// cleared otherwise. The second conjunct fixes every unselected record byte-for-byte.
pub open spec fn cancellation_final_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
) -> bool {
    cancellation_updates_match(before, after, reservations, affected)
        && unselected_work_unchanged(before, after, affected)
}

/// Complete state and result relation exposed by the production update loop.
pub open spec fn cancellation_apply_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    affected: Seq<WorkId>,
    complete: bool,
) -> bool {
    &&& complete == all_targets_exist(before.spec_work(), affected)
    &&& cancellation_result_matches(
        before.spec_work(),
        after.spec_work(),
        before.spec_reservations(),
        affected,
        complete,
    )
    &&& mutation::work_update_preserves_other_state(before, after)
    &&& work_id_layout_matches(before.spec_work(), after.spec_work())
    &&& before.spec_reservation_reducer_ready() ==> after.spec_reservation_reducer_ready()
    &&& before.spec_collections_ordered() ==> after.spec_collections_ordered()
    &&& before.spec_reservation_invariant() ==> after.spec_reservation_invariant()
    &&& before.spec_reservation_reducer_ready()
            && crate::state::queue::queue_bound(before)
        ==> crate::state::queue::queue_bound(after)
    &&& complete ==> cancellation_final_matches(
        before.spec_work(),
        after.spec_work(),
        before.spec_reservations(),
        affected,
    )
}

/// Applies the reservation-sensitive lifecycle mutation for one selected identity.
fn update_one(state: &mut SchedulerState, id: WorkId) -> (updated: bool)
    ensures
        old(state).spec_reservation_reducer_ready() ==> final(state).spec_reservation_reducer_ready(),
        updated == target_exists(old(state).spec_work(), id),
        updated ==> cancellation_step_matches(
            old(state).spec_work(),
            final(state).spec_work(),
            old(state).spec_reservations(),
            id,
        ),
        !updated ==> final(state).spec_work() == old(state).spec_work(),
        work_id_layout_matches(old(state).spec_work(), final(state).spec_work()),
        mutation::work_update_preserves_other_state(old(state), final(state)),
        old(state).spec_reservation_invariant() ==>
            final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    let active = is_active(state.reservations(), id);
    let ghost before = state.spec_work();
    let ghost reservations = state.spec_reservations();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    proof {
        if target_exists(before, id) {
            cancellation_update_admissible(before, reservations, id);
        }
    }
    let updated = if active {
        mutation::set_work_phase(state, id, WorkPhase::Cancelling)
    } else {
        mutation::terminalize_work(state, id, WorkTerminal::Cancelled)
    };
    proof {
        assert(active == has_active_reservation(reservations, id));
        if active {
            reveal(mutation::work_phase_update_matches);
            if updated {
                assert(cancellation_step_matches(before, state.spec_work(), reservations, id));
                cancellation_step_preserves_ids(before, state.spec_work(), reservations, id);
            } else {
                assert(state.spec_work() == before);
            }
        } else {
            reveal(mutation::work_terminal_update_matches);
            if updated {
                assert(cancellation_step_matches(before, state.spec_work(), reservations, id));
                cancellation_step_preserves_ids(before, state.spec_work(), reservations, id);
            } else {
                assert(state.spec_work() == before);
            }
        }
        if updated {
            cancellation_step_target_exists(before, state.spec_work(), reservations, id);
        } else {
            reveal(target_exists);
        }
        if !updated {
            reveal(work_id_layout_matches);
            if was_ready { reveal(SchedulerState::spec_reservation_reducer_ready); }
        }
        if was_ready && had_queue_bound {
            if updated {
                super::queue::cancellation_step_preserves_queue_bound(
                    old(state), state, id,
                );
            } else {
                super::queue::unchanged_work_preserves_queue_bound(old(state), state);
            }
        }
    }
    updated
}

/// Applies cancellation to each already-selected identity in order.
///
/// `false` means the first missing identity was encountered after applying the exact successful
/// prefix, matching the former inline reducer loop. Callers need no proof preconditions. When all
/// supplied identities came from retained work, the result is necessarily `true`.
pub(super) fn apply_updates(
    state: &mut SchedulerState,
    affected: &[WorkId],
) -> (complete: bool)
    ensures cancellation_apply_matches(old(state), final(state), affected@, complete),
{
    let ghost before = state.spec_work();
    let ghost reservations = state.spec_reservations();
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    let mut index = 0;
    proof {
        empty_updates(before, reservations);
        reveal(work_id_layout_matches);
        reveal(unselected_work_unchanged);
        reveal(mutation::work_update_preserves_other_state);
    }
    while index < affected.len()
        invariant
            index <= affected.len(),
            before == old(state).spec_work(),
            reservations == old(state).spec_reservations(),
            state.spec_reservations() == reservations,
            had_invariant == old(state).spec_reservation_invariant(),
            had_invariant ==> state.spec_reservation_invariant(),
            was_ready == old(state).spec_reservation_reducer_ready(),
            was_ready ==> state.spec_reservation_reducer_ready(),
            had_queue_bound == crate::state::queue::queue_bound(old(state)),
            was_ready && had_queue_bound ==> crate::state::queue::queue_bound(state),
            mutation::work_update_preserves_other_state(old(state), state),
            cancellation_updates_match(
                before,
                state.spec_work(),
                reservations,
                affected@.take(index as int),
            ),
            work_id_layout_matches(before, state.spec_work()),
            forall |prior: int| #![trigger affected@[prior]] 0 <= prior < index ==>
                target_exists(before, affected@[prior]),
            unselected_work_unchanged(
                before,
                state.spec_work(),
                affected@.take(index as int),
            ),
        decreases affected.len() - index,
    {
        let id = affected[index];
        let ghost prior_work = state.spec_work();
        let updated = update_one(state, id);
        if !updated {
            proof {
                assert(state.spec_work() == prior_work);
                target_existence_preserved(before, prior_work, id);
                assert(!target_exists(before, id));
                establish_missing_result(
                    before,
                    state.spec_work(),
                    reservations,
                    affected@,
                    index as int,
                );
                reveal(mutation::work_update_preserves_other_state);
                collection_order_preserved(old(state), state);
                reveal(cancellation_apply_matches);
            }
            return false;
        }
        proof {
            advance_prefix(
                before,
                prior_work,
                state.spec_work(),
                reservations,
                affected@,
                index as int,
            );
            assert forall |prior: int| #![trigger affected@[prior]]
                0 <= prior < index + 1 implies target_exists(before, affected@[prior]) by {
                if prior == index {
                    assert(target_exists(before, affected@[index as int]));
                }
            }
            reveal(mutation::work_update_preserves_other_state);
        }
        index += 1;
    }
    proof {
        assert(affected@.take(index as int) =~= affected@);
        assert(all_targets_exist(before, affected@));
        establish_complete_result(before, state.spec_work(), reservations, affected@);
        collection_order_preserved(old(state), state);
        reveal(cancellation_apply_matches);
    }
    true
}

} // verus!
