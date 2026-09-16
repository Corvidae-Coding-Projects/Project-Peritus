//! Finite execution of the exact precomputed worker-loss release plan.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::{DispatchId, LossOutcome, SchedulerState};

use super::super::release_one::{LossReleaseError, release_one};
#[cfg(verus_only)]
use super::super::release_one::{dispatch_exists, loss_release_matches};
use super::WorkerLossError;
#[cfg(verus_only)]
use super::trace::{
    advance_release_trace, empty_release_trace, pending_dispatches_survive,
    release_step_preserves_workers,
};
#[cfg(verus_only)]
use super::{plan_dispatches, release_trace_matches};

verus! {

pub(super) open spec fn planned_dispatches_exist(
    state: &SchedulerState,
    plan: Seq<(DispatchId, Sha256Digest)>,
) -> bool {
    forall |index: int| #![trigger plan[index]] 0 <= index < plan.len() ==>
        dispatch_exists(state, plan[index].0)
}

pub(super) fn release_plan(
    state: &mut SchedulerState,
    plan: &[(DispatchId, Sha256Digest)],
) -> (result: Result<Vec<LossOutcome>, WorkerLossError>)
    ensures
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && plan_dispatches(plan@).no_duplicates()
                && planned_dispatches_exist(old(state), plan@)
            ==> match result {
                Ok(ref outcomes) => {
                    &&& release_trace_matches(old(state), final(state), plan@, outcomes@)
                    &&& final(state).spec_reservation_reducer_ready()
                    &&& final(state).spec_collections_ordered()
                    &&& final(state).spec_workers() == old(state).spec_workers()
                },
                Err(_) => false,
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && plan_dispatches(plan@).no_duplicates()
                && planned_dispatches_exist(old(state), plan@)
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    let ghost initial = *state;
    let ghost valid = state.spec_reservation_reducer_ready()
        && state.spec_collections_ordered()
        && plan_dispatches(plan@).no_duplicates()
        && planned_dispatches_exist(state, plan@);
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    let mut outcomes = Vec::with_capacity(plan.len());
    let mut index = 0;
    proof {
        if valid {
            empty_release_trace(initial);
        }
    }
    while index < plan.len()
        invariant
            index <= plan.len(),
            initial == *old(state),
            outcomes.len() == index,
            valid == (
                initial.spec_reservation_reducer_ready()
                    && initial.spec_collections_ordered()
                    && plan_dispatches(plan@).no_duplicates()
                    && planned_dispatches_exist(&initial, plan@)
            ),
            had_queue_bound == crate::state::queue::queue_bound(&initial),
            valid ==> release_trace_matches(
                &initial,
                state,
                plan@.take(index as int),
                outcomes@,
            ),
            valid ==> plan_dispatches(plan@).no_duplicates(),
            valid ==> planned_dispatches_exist(&initial, plan@),
            valid ==> state.spec_reservation_reducer_ready(),
            valid ==> state.spec_collections_ordered(),
            valid ==> state.spec_workers() == initial.spec_workers(),
            valid && had_queue_bound ==> crate::state::queue::queue_bound(state),
            valid ==> forall |pending: int| #![trigger plan@[pending]]
                index <= pending < plan.len() ==> dispatch_exists(state, plan@[pending].0),
        decreases plan.len() - index,
    {
        let (dispatch_id, failure_digest) = plan[index];
        let ghost before_step = *state;
        let outcome = match release_one(state, dispatch_id, failure_digest) {
            Ok(outcome) => outcome,
            Err(error) => {
                proof { if valid { assert(false); } }
                return Err(match error {
                    LossReleaseError::ReservationDisappeared => {
                        WorkerLossError::ReservationDisappeared
                    },
                    LossReleaseError::WorkDisappeared => WorkerLossError::WorkDisappeared,
                });
            },
        };
        proof {
            if valid {
                assert(loss_release_matches(
                    &before_step, state, dispatch_id, failure_digest, outcome,
                ));
                pending_dispatches_survive(&before_step, state, plan@, index as int, outcome);
                release_step_preserves_workers(
                    &before_step, state, dispatch_id, failure_digest, outcome,
                );
            }
        }
        outcomes.push(outcome);
        proof {
            if valid {
                advance_release_trace(
                    &initial, &before_step, state, plan@, outcomes@, index as int,
                );
            }
        }
        index += 1;
    }
    proof { assert(plan@.take(index as int) =~= plan@); }
    Ok(outcomes)
}

} // verus!
