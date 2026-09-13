//! Cancellation, pause/drain, abandonment, exhaustion, and terminal control.

use crate::state::mutation;
use crate::{
    SchedulerError, SchedulerErrorKind, SchedulerEventKind, SchedulerState, SchedulerTerminal,
    WorkId,
};

pub(super) fn cancel(
    state: &mut SchedulerState,
    work_id: WorkId,
    descendants: bool,
) -> Result<SchedulerEventKind, SchedulerError> {
    match super::cancellation::command::apply_command(state, work_id, descendants) {
        Ok(event) => Ok(event),
        Err(super::cancellation::command::CancellationRejection::WorkNotRetained) => {
            Err(unknown("work is not retained"))
        }
        Err(super::cancellation::command::CancellationRejection::WorkAlreadyTerminal) => {
            Err(crate::reducer::illegal("work is already terminal"))
        }
    }
}

pub(super) fn acknowledge_cancel(
    state: &mut SchedulerState,
    command: &crate::SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    let crate::SchedulerCommandKind::AcknowledgeCancellation { dispatch_id } = command else {
        return Err(crate::reducer::illegal(
            "cancellation acknowledgement dispatcher received a different command",
        ));
    };
    match mutation::apply_acknowledge_cancellation_command(state, command) {
        mutation::AcknowledgeCancellationOutcome::Applied => {
            Ok(SchedulerEventKind::CancellationAcknowledged { dispatch_id: *dispatch_id })
        }
        mutation::AcknowledgeCancellationOutcome::DispatchNotActive => {
            Err(unknown("dispatch is not active"))
        }
        mutation::AcknowledgeCancellationOutcome::ReservationWorkDisappeared => {
            Err(unknown("reservation work disappeared"))
        }
        mutation::AcknowledgeCancellationOutcome::WorkNotCancelling => {
            Err(crate::reducer::illegal("dispatch work is not cancelling"))
        }
        mutation::AcknowledgeCancellationOutcome::CancellingWorkDisappeared => {
            Err(unknown("cancelling work disappeared"))
        }
        mutation::AcknowledgeCancellationOutcome::NotAcknowledgeCancellationCommand => {
            Err(crate::reducer::illegal(
                "cancellation acknowledgement dispatcher received a different command",
            ))
        }
    }
}

pub(super) fn exhaust(
    state: &mut SchedulerState,
    command: &crate::SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    let crate::SchedulerCommandKind::ExhaustWork { work_id, cause_digest } = command else {
        return Err(crate::reducer::illegal("exhaust dispatcher received a non-exhaust command"));
    };
    match mutation::apply_exhaust_command(state, command) {
        mutation::ExhaustCommandOutcome::Applied => {
            Ok(SchedulerEventKind::WorkExhausted { work_id: *work_id, cause_digest: *cause_digest })
        }
        mutation::ExhaustCommandOutcome::WorkNotRetained
        | mutation::ExhaustCommandOutcome::WorkDisappeared => Err(unknown("work is not retained")),
        mutation::ExhaustCommandOutcome::WorkNotExhaustible => {
            Err(crate::reducer::illegal("active or terminal work cannot be explicitly exhausted"))
        }
        mutation::ExhaustCommandOutcome::NotExhaustCommand => {
            Err(crate::reducer::illegal("exhaust dispatcher received a non-exhaust command"))
        }
    }
}

pub(super) fn abandon(
    state: &mut SchedulerState,
    command: &crate::SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    let crate::SchedulerCommandKind::AbandonDispatch { dispatch_id, cause_digest } = command else {
        return Err(crate::reducer::illegal("abandonment dispatcher received a different command"));
    };
    match mutation::apply_abandon_command(state, command) {
        mutation::AbandonCommandOutcome::Applied => Ok(SchedulerEventKind::DispatchAbandoned {
            dispatch_id: *dispatch_id,
            cause_digest: *cause_digest,
        }),
        mutation::AbandonCommandOutcome::DispatchNotActive => {
            Err(unknown("dispatch is not active"))
        }
        mutation::AbandonCommandOutcome::WorkDisappeared => {
            Err(unknown("abandoned work disappeared"))
        }
        mutation::AbandonCommandOutcome::NotAbandonCommand => {
            Err(crate::reducer::illegal("abandonment dispatcher received a different command"))
        }
    }
}

pub(super) fn scheduler_phase(
    state: &mut SchedulerState,
    command: &crate::SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    match mutation::apply_phase_command(state, command) {
        mutation::PhaseCommandOutcome::Applied => match command {
            crate::SchedulerCommandKind::PauseScheduler => Ok(SchedulerEventKind::SchedulerPaused),
            crate::SchedulerCommandKind::ResumeScheduler => {
                Ok(SchedulerEventKind::SchedulerResumed)
            }
            crate::SchedulerCommandKind::DrainScheduler => {
                Ok(SchedulerEventKind::SchedulerDrainRequested)
            }
            _ => Err(crate::reducer::illegal(
                "scheduler phase dispatcher received a non-phase command",
            )),
        },
        mutation::PhaseCommandOutcome::IllegalPause => {
            Err(crate::reducer::illegal("scheduler is already paused or terminal"))
        }
        mutation::PhaseCommandOutcome::IllegalResume => {
            Err(crate::reducer::illegal("scheduler is not paused"))
        }
        mutation::PhaseCommandOutcome::IllegalDrain => {
            Err(crate::reducer::illegal("scheduler is already draining"))
        }
        mutation::PhaseCommandOutcome::NotPhaseCommand => {
            Err(crate::reducer::illegal("scheduler phase dispatcher received a non-phase command"))
        }
    }
}

pub(super) fn finalize(state: &mut SchedulerState) -> Result<SchedulerEventKind, SchedulerError> {
    if !state.all_work_terminal() || !state.reservations().is_empty() {
        return Err(crate::reducer::illegal(
            "scheduler cannot finalize with nonterminal work or directives",
        ));
    }
    let terminal = SchedulerTerminal::evaluate(state.work());
    mutation::set_terminal(state, terminal.clone());
    Ok(SchedulerEventKind::SchedulerFinalized { terminal })
}

fn unknown(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::UnknownIdentity, detail)
}
