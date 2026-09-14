//! Complete scheduler-state frame across a worker-loss release trace.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::{DispatchId, LossOutcome, SchedulerState};

use super::super::release_one::loss_release_matches;
use super::trace::release_step_preserves_batch_state;
use super::{
    release_batch_preserves_other_state, release_trace_matches, worker_loss_preserves_other_state,
};

verus! {

proof fn release_step_preserves_batch_frame(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
)
    requires loss_release_matches(before, after, dispatch_id, failure_digest, outcome),
    ensures release_batch_preserves_other_state(before, after),
{
    release_step_preserves_batch_state(before, after, dispatch_id, failure_digest, outcome);
}

proof fn batch_frame_transitive(
    before: &SchedulerState,
    middle: &SchedulerState,
    after: &SchedulerState,
)
    requires
        release_batch_preserves_other_state(before, middle),
        release_batch_preserves_other_state(middle, after),
    ensures release_batch_preserves_other_state(before, after),
{
    reveal(release_batch_preserves_other_state);
    reveal(worker_loss_preserves_other_state);
}

proof fn history_preserves_batch_frame(
    states: Seq<SchedulerState>,
    plan: Seq<(DispatchId, Sha256Digest)>,
    outcomes: Seq<LossOutcome>,
    step: int,
)
    requires
        states.len() == plan.len() + 1,
        outcomes.len() == plan.len(),
        0 <= step <= plan.len(),
        forall |at: int| #![trigger states[at]] 0 <= at < plan.len() ==>
            loss_release_matches(
                &states[at],
                &states[at + 1],
                plan[at].0,
                plan[at].1,
                outcomes[at],
            ),
    ensures release_batch_preserves_other_state(&states[0], &states[step]),
    decreases step,
{
    if step == 0 {
        reveal(release_batch_preserves_other_state);
        reveal(worker_loss_preserves_other_state);
    } else {
        history_preserves_batch_frame(states, plan, outcomes, step - 1);
        release_step_preserves_batch_frame(
            &states[step - 1],
            &states[step],
            plan[step - 1].0,
            plan[step - 1].1,
            outcomes[step - 1],
        );
        batch_frame_transitive(&states[0], &states[step - 1], &states[step]);
    }
}

pub(super) proof fn release_trace_preserves_batch_frame(
    before: &SchedulerState,
    after: &SchedulerState,
    plan: Seq<(DispatchId, Sha256Digest)>,
    outcomes: Seq<LossOutcome>,
)
    requires release_trace_matches(before, after, plan, outcomes),
    ensures release_batch_preserves_other_state(before, after),
{
    reveal(release_trace_matches);
    let states = choose |states: Seq<SchedulerState>| {
        &&& outcomes.len() == plan.len()
        &&& states.len() == plan.len() + 1
        &&& states[0] == *before
        &&& states[plan.len() as int] == *after
        &&& forall |step: int| #![trigger states[step]] 0 <= step < plan.len() ==>
            loss_release_matches(
                &states[step],
                &states[step + 1],
                plan[step].0,
                plan[step].1,
                outcomes[step],
            )
    };
    history_preserves_batch_frame(states, plan, outcomes, plan.len() as int);
}

} // verus!
