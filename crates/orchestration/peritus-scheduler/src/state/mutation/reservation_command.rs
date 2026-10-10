//! Verified production command kernels for live reservation transitions.

mod acknowledge_cancellation;
mod complete;

mod outcome;

pub use acknowledge_cancellation::apply_acknowledge_cancellation_command;
pub use complete::apply_complete_command;

pub use outcome::{
    AbandonCommandOutcome, AcknowledgeCancellationOutcome, AcknowledgeStartOutcome,
    CompleteCommandOutcome, FailCommandOutcome,
};

use crate::{FailureDisposition, SchedulerCommandKind, SchedulerState, WorkPhase, WorkTerminal};

#[doc = " Applies the actual `AcknowledgeStart` payload through the verified atomic mutation."]
pub fn apply_acknowledge_start_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> AcknowledgeStartOutcome {
    let dispatch_id = match command {
        SchedulerCommandKind::AcknowledgeStart { dispatch_id } => *dispatch_id,
        _ => return AcknowledgeStartOutcome::NotAcknowledgeStartCommand,
    };
    let Some(reservation) = state.reservation(dispatch_id) else {
        return AcknowledgeStartOutcome::DispatchNotActive;
    };
    let work_id = reservation.work_id();
    let started = reservation.started();
    if started {
        return AcknowledgeStartOutcome::AlreadyAcknowledged;
    }
    match super::acknowledge_reservation_start(state, dispatch_id, work_id) {
        Some(true) => AcknowledgeStartOutcome::Applied,
        Some(false) => AcknowledgeStartOutcome::WorkDisappeared,
        None => AcknowledgeStartOutcome::DispatchDisappeared,
    }
}
#[doc = " Applies the actual `FailWork` payload through the verified release mutation."]
pub fn apply_fail_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> FailCommandOutcome {
    let (dispatch_id, failure_digest, disposition) = match command {
        SchedulerCommandKind::FailWork { dispatch_id, failure_digest, disposition } => {
            (*dispatch_id, *failure_digest, *disposition)
        }
        _ => return FailCommandOutcome::NotFailCommand,
    };
    let Some(reservation) = state.reservation(dispatch_id) else {
        return FailCommandOutcome::DispatchNotActive;
    };
    let work_id = reservation.work_id();
    let Some(work) = state.work_item(work_id) else {
        return FailCommandOutcome::WorkDisappeared;
    };
    let phase = work.phase();
    let attempts_started = work.attempts_started();
    let maximum_attempts = work.spec().maximum_attempts().get();
    if !phase.same(WorkPhase::Reserved) && !phase.same(WorkPhase::Running) {
        return FailCommandOutcome::WorkNotFailable;
    }
    let released = match disposition {
        FailureDisposition::Retryable if attempts_started < maximum_attempts => {
            super::release_to_retry_pending(state, dispatch_id, work_id, failure_digest)
        }
        FailureDisposition::Retryable => super::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Exhausted { cause_digest: failure_digest },
        ),
        FailureDisposition::Failed => super::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Failed { failure_digest },
        ),
        FailureDisposition::Ambiguous => super::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Ambiguous { dispatch_id },
        ),
    };
    if released.is_some() {
        FailCommandOutcome::Applied
    } else {
        FailCommandOutcome::WorkDisappeared
    }
}
#[doc = " Applies the actual `AbandonDispatch` payload through the verified release mutation."]
pub fn apply_abandon_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> AbandonCommandOutcome {
    let (dispatch_id, cause_digest) = match command {
        SchedulerCommandKind::AbandonDispatch { dispatch_id, cause_digest } => {
            (*dispatch_id, *cause_digest)
        }
        _ => return AbandonCommandOutcome::NotAbandonCommand,
    };
    let Some(reservation) = state.reservation(dispatch_id) else {
        return AbandonCommandOutcome::DispatchNotActive;
    };
    let work_id = reservation.work_id();
    let phase = match state.work_item(work_id) {
        Some(work) => work.phase(),
        None => return AbandonCommandOutcome::WorkDisappeared,
    };
    let terminal = if phase.same(WorkPhase::Cancelling) {
        WorkTerminal::Cancelled
    } else {
        WorkTerminal::Abandoned { cause_digest }
    };
    if super::release_to_terminal(state, dispatch_id, work_id, terminal).is_some() {
        AbandonCommandOutcome::Applied
    } else {
        AbandonCommandOutcome::WorkDisappeared
    }
}
