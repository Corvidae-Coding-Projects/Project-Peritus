//! Verified production command kernels for standalone work lifecycle changes.

use crate::{SchedulerCommandKind, SchedulerState, WorkPhase, WorkTerminal};

#[doc = " Deterministic result of applying the actual retry command payload."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryCommandOutcome {
    #[doc = " Retry-pending work returned to the queue."]
    Applied,
    #[doc = " The actual command was not `RetryWork`."]
    NotRetryCommand,
    #[doc = " The target work identity was not retained."]
    WorkNotRetained,
    #[doc = " Retained target work was not retry-pending."]
    WorkNotRetryPending,
    #[doc = " The target exhausted its immutable attempt bound."]
    AttemptBoundExhausted,
    #[doc = " The target disappeared between lookup and update."]
    WorkDisappeared,
}
#[doc = " Deterministic result of applying the actual exhaust command payload."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExhaustCommandOutcome {
    #[doc = " Inactive work became terminally exhausted."]
    Applied,
    #[doc = " The actual command was not `ExhaustWork`."]
    NotExhaustCommand,
    #[doc = " The target work identity was not retained."]
    WorkNotRetained,
    #[doc = " Active or terminal work cannot be explicitly exhausted."]
    WorkNotExhaustible,
    #[doc = " The target disappeared between lookup and update."]
    WorkDisappeared,
}
#[doc = " Applies the actual `RetryWork` payload through the verified work-update kernel."]
pub fn apply_retry_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> RetryCommandOutcome {
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
    if super::queue_work_retry(state, work_id) {
        RetryCommandOutcome::Applied
    } else {
        RetryCommandOutcome::WorkDisappeared
    }
}
#[doc = " Applies the actual `ExhaustWork` payload through the verified work-update kernel."]
pub fn apply_exhaust_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> ExhaustCommandOutcome {
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
    if super::terminalize_work(state, work_id, WorkTerminal::Exhausted { cause_digest }) {
        ExhaustCommandOutcome::Applied
    } else {
        ExhaustCommandOutcome::WorkDisappeared
    }
}
