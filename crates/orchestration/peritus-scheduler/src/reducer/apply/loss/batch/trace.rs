//! Proof composition for the ordered worker-loss release trace.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::{
    DispatchId, LossOutcome, SchedulerEventKind, SchedulerReservation, SchedulerState, WorkerId,
    WorkerPhase,
};

use super::super::release_one::{dispatch_exists, loss_release_matches, release_effect_matches};
use super::state_frame::release_trace_preserves_batch_frame;
use super::{
    plan_dispatches, release_batch_preserves_other_state, release_trace_matches,
    worker_loss_apply_matches, worker_loss_event_matches, worker_loss_preserves_other_state,
};

verus! {

pub(super) proof fn empty_release_trace(state: SchedulerState)
    ensures release_trace_matches(
        &state,
        &state,
        Seq::<(DispatchId, Sha256Digest)>::empty(),
        Seq::<LossOutcome>::empty(),
    ),
{
    let states = Seq::<SchedulerState>::empty().push(state);
    reveal(release_trace_matches);
    assert(states.len() == 1);
    assert(states[0] == state);
}

pub(super) proof fn advance_release_trace(
    initial: &SchedulerState,
    middle: &SchedulerState,
    after: &SchedulerState,
    plan: Seq<(DispatchId, Sha256Digest)>,
    outcomes: Seq<LossOutcome>,
    index: int,
)
    requires
        0 <= index < plan.len(),
        outcomes.len() == index + 1,
        release_trace_matches(
            initial,
            middle,
            plan.take(index),
            outcomes.take(index),
        ),
        loss_release_matches(
            middle,
            after,
            plan[index].0,
            plan[index].1,
            outcomes[index],
        ),
    ensures release_trace_matches(
        initial,
        after,
        plan.take(index + 1),
        outcomes,
    ),
{
    reveal(release_trace_matches);
    let prior = choose |states: Seq<SchedulerState>| {
        &&& states.len() == plan.take(index).len() + 1
        &&& states[0] == *initial
        &&& states[plan.take(index).len() as int] == *middle
        &&& forall |step: int| #![trigger states[step]]
            0 <= step < plan.take(index).len() ==>
                loss_release_matches(
                    &states[step],
                    &states[step + 1],
                    plan.take(index)[step].0,
                    plan.take(index)[step].1,
                    outcomes.take(index)[step],
                )
    };
    let states = prior.push(*after);
    assert(plan.take(index).len() == index);
    assert(plan.take(index + 1) =~= plan.take(index).push(plan[index]));
    assert(outcomes =~= outcomes.take(index).push(outcomes[index]));
    assert forall |step: int| #![trigger states[step]]
        0 <= step < plan.take(index + 1).len() implies
            loss_release_matches(
                &states[step],
                &states[step + 1],
                plan.take(index + 1)[step].0,
                plan.take(index + 1)[step].1,
                outcomes[step],
            ) by {
        if step < index {
            assert(states[step] == prior[step]);
            assert(states[step + 1] == prior[step + 1]);
            assert(plan.take(index + 1)[step] == plan.take(index)[step]);
            assert(outcomes[step] == outcomes.take(index)[step]);
        } else {
            assert(step == index);
            assert(states[step] == *middle);
            assert(states[step + 1] == *after);
        }
    }
    assert(outcomes.len() == plan.take(index + 1).len());
    assert(states.len() == plan.take(index + 1).len() + 1);
    assert(states[0] == *initial);
    assert(states[plan.take(index + 1).len() as int] == *after);
    assert(exists |candidate: Seq<SchedulerState>| {
        &&& outcomes.len() == plan.take(index + 1).len()
        &&& candidate.len() == plan.take(index + 1).len() + 1
        &&& candidate[0] == *initial
        &&& candidate[plan.take(index + 1).len() as int] == *after
        &&& forall |step: int| #![trigger candidate[step]]
            0 <= step < plan.take(index + 1).len() ==>
                loss_release_matches(
                    &candidate[step],
                    &candidate[step + 1],
                    plan.take(index + 1)[step].0,
                    plan.take(index + 1)[step].1,
                    outcomes[step],
                )
    }) by {
        assert(states[plan.take(index + 1).len() as int] == *after);
    }
}

proof fn release_reservation_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
)
    requires loss_release_matches(before, after, dispatch_id, failure_digest, outcome),
    ensures exists |removed: SchedulerReservation| {
        crate::state::mutation::exact_reservation_removal_matches(
            before.spec_reservations(),
            after.spec_reservations(),
            dispatch_id,
            Some(removed),
        )
    },
{
    reveal(loss_release_matches);
    let work_index = choose |work_index: int| #![trigger before.spec_work()[work_index]]
        exists |removed: SchedulerReservation|
            #![trigger release_effect_matches(
                before, after, dispatch_id, failure_digest, outcome, &removed,
            )]
        {
        &&& 0 <= work_index < before.spec_work().len()
        &&& super::super::outcome_matches(before.spec_work()[work_index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )
    };
    let removed = choose |removed: SchedulerReservation| {
        &&& 0 <= work_index < before.spec_work().len()
        &&& super::super::outcome_matches(before.spec_work()[work_index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )
    };
    reveal(release_effect_matches);
    match outcome {
        LossOutcome::Requeued { .. } => reveal(crate::state::mutation::phase_release_matches),
        LossOutcome::Cancelled { .. }
        | LossOutcome::Exhausted { .. }
        | LossOutcome::Ambiguous { .. }
        | LossOutcome::Failed { .. } => {
            reveal(crate::state::mutation::terminal_release_matches);
        },
    }
}

/// Every still-pending distinct dispatch survives the exact current removal.
pub(super) proof fn pending_dispatches_survive(
    before: &SchedulerState,
    after: &SchedulerState,
    plan: Seq<(DispatchId, Sha256Digest)>,
    index: int,
    outcome: LossOutcome,
)
    requires
        0 <= index < plan.len(),
        plan_dispatches(plan).no_duplicates(),
        forall |pending: int| #![trigger plan[pending]] index <= pending < plan.len() ==>
            dispatch_exists(before, plan[pending].0),
        loss_release_matches(
            before, after, plan[index].0, plan[index].1, outcome,
        ),
    ensures forall |pending: int| #![trigger plan[pending]] index + 1 <= pending < plan.len() ==>
        dispatch_exists(after, plan[pending].0),
{
    release_reservation_matches(before, after, plan[index].0, plan[index].1, outcome);
    let removed = choose |removed: SchedulerReservation| {
        crate::state::mutation::exact_reservation_removal_matches(
            before.spec_reservations(),
            after.spec_reservations(),
            plan[index].0,
            Some(removed),
        )
    };
    reveal(crate::state::mutation::exact_reservation_removal_matches);
    let removed_at = choose |at: int| #![trigger before.spec_reservations()[at]] {
        &&& 0 <= at < before.spec_reservations().len()
        &&& before.spec_reservations()[at].spec_dispatch_id() == plan[index].0
        &&& removed == before.spec_reservations()[at]
        &&& after.spec_reservations() == before.spec_reservations().remove(at)
    };
    reveal(plan_dispatches);
    reveal(Seq::no_duplicates);
    assert forall |pending: int| #![trigger plan[pending]]
        index + 1 <= pending < plan.len() implies
            dispatch_exists(after, plan[pending].0) by {
        let dispatches = plan_dispatches(plan);
        assert(dispatches[pending] != dispatches[index]);
        assert(dispatches[pending] == plan[pending].0);
        assert(dispatches[index] == plan[index].0);
        reveal(dispatch_exists);
        let retained_at = choose |at: int| #![trigger before.spec_reservations()[at]]
            0 <= at < before.spec_reservations().len()
                && before.spec_reservations()[at].spec_dispatch_id() == plan[pending].0;
        assert(retained_at != removed_at);
        let after_at = if retained_at < removed_at { retained_at } else { retained_at - 1 };
        assert(0 <= after_at < after.spec_reservations().len());
        assert(after.spec_reservations()[after_at] == before.spec_reservations()[retained_at]);
    }
}

pub(super) proof fn release_step_preserves_batch_state(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
)
    requires loss_release_matches(before, after, dispatch_id, failure_digest, outcome),
    ensures
        after.spec_workers() == before.spec_workers(),
        release_batch_preserves_other_state(before, after),
{
    reveal(loss_release_matches);
    let work_index = choose |work_index: int| #![trigger before.spec_work()[work_index]]
        exists |removed: SchedulerReservation|
            #![trigger release_effect_matches(
                before, after, dispatch_id, failure_digest, outcome, &removed,
            )]
        {
        &&& 0 <= work_index < before.spec_work().len()
        &&& super::super::outcome_matches(before.spec_work()[work_index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )
    };
    let removed = choose |removed: SchedulerReservation| {
        &&& 0 <= work_index < before.spec_work().len()
        &&& super::super::outcome_matches(before.spec_work()[work_index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )
    };
    reveal(release_effect_matches);
    match outcome {
        LossOutcome::Requeued { .. } => reveal(crate::state::mutation::phase_release_matches),
        LossOutcome::Cancelled { .. }
        | LossOutcome::Exhausted { .. }
        | LossOutcome::Ambiguous { .. }
        | LossOutcome::Failed { .. } => {
            reveal(crate::state::mutation::terminal_release_matches);
        },
    }
    reveal(release_batch_preserves_other_state);
    reveal(worker_loss_preserves_other_state);
}

pub(super) proof fn release_step_preserves_workers(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
)
    requires loss_release_matches(before, after, dispatch_id, failure_digest, outcome),
    ensures after.spec_workers() == before.spec_workers(),
{
    release_step_preserves_batch_state(before, after, dispatch_id, failure_digest, outcome);
}

pub(super) proof fn establish_worker_loss_apply(
    before: &SchedulerState,
    released: &SchedulerState,
    after: &SchedulerState,
    worker_id: WorkerId,
    plan: Seq<(DispatchId, Sha256Digest)>,
    event: &SchedulerEventKind,
)
    requires
        release_trace_matches(
            before,
            released,
            plan,
            match event {
                SchedulerEventKind::WorkerLost { outcomes, .. } => outcomes@,
                _ => Seq::<LossOutcome>::empty(),
            },
        ),
        crate::state::mutation::worker_phase_state_matches(
            released, after, worker_id, WorkerPhase::Lost, true,
        ),
        worker_loss_event_matches(
            event,
            worker_id,
            match event {
                SchedulerEventKind::WorkerLost { outcomes, .. } => outcomes@,
                _ => Seq::<LossOutcome>::empty(),
            },
        ),
    ensures worker_loss_apply_matches(before, after, worker_id, plan, event),
{
    let outcomes = match event {
        SchedulerEventKind::WorkerLost { outcomes, .. } => outcomes@,
        _ => Seq::<LossOutcome>::empty(),
    };
    release_trace_preserves_batch_frame(before, released, plan, outcomes);
    reveal(release_batch_preserves_other_state);
    reveal(worker_loss_preserves_other_state);
    reveal(crate::state::mutation::worker_phase_state_matches);
    reveal(crate::state::mutation::worker_update_preserves_other_state);
    assert(worker_loss_preserves_other_state(before, after));
    reveal(worker_loss_apply_matches);
}

} // verus!
