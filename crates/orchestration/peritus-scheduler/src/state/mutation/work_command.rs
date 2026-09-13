//! Verified production command kernels for standalone work lifecycle changes.

use vstd::prelude::*;

use crate::{SchedulerCommandKind, SchedulerState, WorkPhase, WorkTerminal};

verus! {

/// Deterministic result of applying the actual retry command payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryCommandOutcome {
    /// Retry-pending work returned to the queue.
    Applied,
    /// The actual command was not `RetryWork`.
    NotRetryCommand,
    /// The target work identity was not retained.
    WorkNotRetained,
    /// Retained target work was not retry-pending.
    WorkNotRetryPending,
    /// The target exhausted its immutable attempt bound.
    AttemptBoundExhausted,
    /// The target disappeared between lookup and update.
    WorkDisappeared,
}

/// Deterministic result of applying the actual exhaust command payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExhaustCommandOutcome {
    /// Inactive work became terminally exhausted.
    Applied,
    /// The actual command was not `ExhaustWork`.
    NotExhaustCommand,
    /// The target work identity was not retained.
    WorkNotRetained,
    /// Active or terminal work cannot be explicitly exhausted.
    WorkNotExhaustible,
    /// The target disappeared between lookup and update.
    WorkDisappeared,
}

/// Applies the actual `RetryWork` payload through the verified work-update kernel.
pub fn apply_retry_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> (outcome: RetryCommandOutcome)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        outcome != RetryCommandOutcome::Applied ==>
            final(state).spec_work() == old(state).spec_work(),
        match command {
            SchedulerCommandKind::RetryWork { work_id } => {
                outcome == RetryCommandOutcome::Applied ==> super::work_update::work_update_matches(
                    old(state).spec_work(),
                    final(state).spec_work(),
                    *work_id,
                    WorkPhase::Queued,
                    true,
                )
            },
            _ => outcome == RetryCommandOutcome::NotRetryCommand,
        },
{
    let work_id = match command {
        SchedulerCommandKind::RetryWork { work_id } => *work_id,
        _ => return RetryCommandOutcome::NotRetryCommand,
    };
    let Some(work) = state.work_item(work_id) else {
        return RetryCommandOutcome::WorkNotRetained;
    };
    let phase = work.phase();
    let attempts_started = work.attempts_started();
    let maximum_attempts = work.spec().maximum_attempts().get();
    if !phase.same(WorkPhase::RetryPending) {
        return RetryCommandOutcome::WorkNotRetryPending;
    }
    if attempts_started >= maximum_attempts {
        return RetryCommandOutcome::AttemptBoundExhausted;
    }
    proof {
        if state.spec_reservation_reducer_ready() {
            let work_index = choose |index: int| #![trigger state.spec_work()[index]]
                0 <= index < state.spec_work().len()
                    && state.spec_work()[index].spec_definition().spec_id() == work_id
                    && state.spec_work()[index].spec_phase() == phase;
            reveal(crate::verified::work_phase_retains_reservation);
            crate::verified::inactive_work_update_is_admissible(
                state,
                work_index,
                WorkPhase::Queued,
            );
        }
    }
    if super::queue_work_retry(state, work_id) {
        RetryCommandOutcome::Applied
    } else {
        RetryCommandOutcome::WorkDisappeared
    }
}

/// Applies the actual `ExhaustWork` payload through the verified work-update kernel.
pub fn apply_exhaust_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> (outcome: ExhaustCommandOutcome)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        outcome != ExhaustCommandOutcome::Applied ==>
            final(state).spec_work() == old(state).spec_work(),
        match command {
            SchedulerCommandKind::ExhaustWork { work_id, .. } => {
                outcome == ExhaustCommandOutcome::Applied ==> super::work_update::work_update_matches(
                    old(state).spec_work(),
                    final(state).spec_work(),
                    *work_id,
                    WorkPhase::Terminal,
                    true,
                )
            },
            _ => outcome == ExhaustCommandOutcome::NotExhaustCommand,
        },
{
    let (work_id, cause_digest) = match command {
        SchedulerCommandKind::ExhaustWork { work_id, cause_digest } => (*work_id, *cause_digest),
        _ => return ExhaustCommandOutcome::NotExhaustCommand,
    };
    let phase = match state.work_item(work_id) {
        Some(record) => record.phase(),
        None => return ExhaustCommandOutcome::WorkNotRetained,
    };
    if phase.same(WorkPhase::Reserved)
        || phase.same(WorkPhase::Running)
        || phase.same(WorkPhase::Cancelling)
        || phase.same(WorkPhase::Terminal)
    {
        return ExhaustCommandOutcome::WorkNotExhaustible;
    }
    proof {
        if state.spec_reservation_reducer_ready() {
            let work_index = choose |index: int| #![trigger state.spec_work()[index]]
                0 <= index < state.spec_work().len()
                    && state.spec_work()[index].spec_definition().spec_id() == work_id
                    && state.spec_work()[index].spec_phase() == phase;
            reveal(crate::verified::work_phase_retains_reservation);
            crate::verified::inactive_work_update_is_admissible(
                state,
                work_index,
                WorkPhase::Terminal,
            );
        }
    }
    if super::terminalize_work(
        state,
        work_id,
        WorkTerminal::Exhausted { cause_digest },
    ) {
        ExhaustCommandOutcome::Applied
    } else {
        ExhaustCommandOutcome::WorkDisappeared
    }
}

} // verus!
