//! Lookup completeness and exact-result projections for one worker-loss release.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::state::mutation;
use crate::{DispatchId, LossOutcome, SchedulerReservation, SchedulerState, WorkId, WorkRecord};

use super::{dispatch_exists, loss_release_matches, outcome_matches, release_effect_matches};

verus! {

pub(super) proof fn missing_dispatch_is_impossible(
    state: &SchedulerState,
    dispatch_id: DispatchId,
)
    requires
        state.spec_collections_ordered(),
        dispatch_exists(state, dispatch_id),
        forall |index: int| 0 <= index < state.spec_reservations().len() ==>
            state.spec_reservations()[index].spec_dispatch_id() != dispatch_id,
    ensures false,
{
    reveal(dispatch_exists);
    let index = choose |index: int| #![trigger state.spec_reservations()[index]]
        0 <= index < state.spec_reservations().len()
            && state.spec_reservations()[index].spec_dispatch_id() == dispatch_id;
    assert(state.spec_reservations()[index].spec_dispatch_id() != dispatch_id);
}

pub(super) proof fn missing_work_is_impossible(
    state: &SchedulerState,
    dispatch_id: DispatchId,
    work_id: WorkId,
)
    requires
        state.spec_reservation_reducer_ready(),
        state.spec_collections_ordered(),
        mutation::release_target_exists(state, dispatch_id, work_id),
        forall |index: int| 0 <= index < state.spec_work().len() ==>
            state.spec_work()[index].spec_definition().spec_id() != work_id,
    ensures false,
{
    reveal(mutation::release_target_exists);
    let reservation_index = choose |index: int| #![trigger state.spec_reservations()[index]]
        0 <= index < state.spec_reservations().len()
            && state.spec_reservations()[index].spec_dispatch_id() == dispatch_id
            && state.spec_reservations()[index].spec_work_id() == work_id;
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservation_invariant_parts);
    reveal(crate::verified::reservations_bind_active_work);
    let work_index = choose |index: int|
        #![trigger state.spec_work()[index].spec_definition().spec_id()]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id()
                == state.spec_reservations()[reservation_index].spec_work_id()
            && crate::verified::work_phase_retains_reservation(
                state.spec_work()[index].spec_phase(),
            );
    assert(state.spec_work()[work_index].spec_definition().spec_id() == work_id);
}

pub(super) proof fn establish_loss_release(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
    removed: &SchedulerReservation,
    work_index: int,
    selected_work: WorkRecord,
)
    requires
        0 <= work_index < before.spec_work().len(),
        before.spec_work()[work_index] == selected_work,
        outcome_matches(selected_work, dispatch_id, outcome),
        release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, removed,
        ),
    ensures loss_release_matches(before, after, dispatch_id, failure_digest, outcome),
{
    reveal(loss_release_matches);
    assert(exists |index: int, released: SchedulerReservation| {
        &&& 0 <= index < before.spec_work().len()
        &&& outcome_matches(before.spec_work()[index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &released,
        )
    }) by {
    }
}

} // verus!
