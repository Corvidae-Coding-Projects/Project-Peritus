//! Cross-record transition binding validation before C0 append.

use crate::{
    SchedulerCommand, SchedulerCommandKind, SchedulerError, SchedulerEventKind, SchedulerTransition,
};

pub fn validate(
    command: &SchedulerCommand,
    transition: &SchedulerTransition,
) -> Result<(), SchedulerError> {
    let event = transition.event();
    let state = transition.state();
    let mismatches = [
        command.event_id() != event.id(),
        command.command_id() != event.command_id(),
        command.run_id() != event.run_id(),
        command.run_id() != state.run_id(),
        command.expected_previous_event() != event.previous_event(),
        command.expected_sequence().checked_add(1) != Some(event.sequence().get()),
        command.revision() != event.revision(),
        state.binding().revision() != event.revision(),
        command.prior_state_digest() != event.prior_state_digest(),
        event.successor_state_digest() != state.state_digest(),
        event.sequence() != state.sequence(),
        event.id() != state.last_event_id(),
        !event_matches_command(command.kind(), event.kind()),
    ];
    if mismatches.into_iter().any(core::convert::identity) {
        return Err(super::binding_error(
            "scheduler command, event, and successor checkpoint bindings differ",
        ));
    }
    Ok(())
}

#[allow(
    clippy::match_same_arms,
    clippy::too_many_lines,
    reason = "the closed command/event table stays contiguous and auditable"
)]
fn event_matches_command(command: &SchedulerCommandKind, event: &SchedulerEventKind) -> bool {
    match (command, event) {
        (
            SchedulerCommandKind::StartScheduler { binding: left },
            SchedulerEventKind::SchedulerStarted { binding: right },
        ) => left == right,
        (
            SchedulerCommandKind::RegisterWorker { descriptor: left },
            SchedulerEventKind::WorkerRegistered { descriptor: right },
        ) => left == right,
        (
            SchedulerCommandKind::SetWorkerAvailable { worker_id: left },
            SchedulerEventKind::WorkerAvailable { worker_id: right },
        )
        | (
            SchedulerCommandKind::DrainWorker { worker_id: left },
            SchedulerEventKind::WorkerDrainRequested { worker_id: right },
        )
        | (
            SchedulerCommandKind::LoseWorker { worker_id: left },
            SchedulerEventKind::WorkerLost { worker_id: right, .. },
        )
        | (
            SchedulerCommandKind::RemoveWorker { worker_id: left },
            SchedulerEventKind::WorkerRemoved { worker_id: right },
        ) => left == right,
        (
            SchedulerCommandKind::AdmitWork { spec: left },
            SchedulerEventKind::WorkAdmitted { spec: right },
        ) => left == right,
        (
            SchedulerCommandKind::DispatchNext { dispatch_id, dispatch_token },
            SchedulerEventKind::WorkReserved { reservation },
        ) => {
            dispatch_id == &reservation.dispatch_id()
                && dispatch_token == &reservation.dispatch_token()
        }
        (
            SchedulerCommandKind::AcknowledgeStart { dispatch_id: left },
            SchedulerEventKind::WorkStartAcknowledged { dispatch_id: right },
        )
        | (
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: left },
            SchedulerEventKind::CancellationAcknowledged { dispatch_id: right },
        ) => left == right,
        (
            SchedulerCommandKind::CompleteWork { dispatch_id: left_id, result_digest: left_digest },
            SchedulerEventKind::WorkSucceeded {
                dispatch_id: right_id,
                result_digest: right_digest,
            },
        ) => left_id == right_id && left_digest == right_digest,
        (
            SchedulerCommandKind::FailWork {
                dispatch_id: left_id,
                failure_digest: left_digest,
                disposition: left_disposition,
            },
            SchedulerEventKind::WorkFailed {
                dispatch_id: right_id,
                failure_digest: right_digest,
                disposition: right_disposition,
            },
        ) => {
            left_id == right_id
                && left_digest == right_digest
                && left_disposition == right_disposition
        }
        (
            SchedulerCommandKind::RetryWork { work_id: left },
            SchedulerEventKind::WorkRetryQueued { work_id: right },
        ) => left == right,
        (
            SchedulerCommandKind::CancelWork { work_id: left },
            SchedulerEventKind::WorkCancelled { work_id: right, descendants: false, .. },
        )
        | (
            SchedulerCommandKind::CancelWorkTree { work_id: left },
            SchedulerEventKind::WorkCancelled { work_id: right, descendants: true, .. },
        ) => left == right,
        (
            SchedulerCommandKind::ExhaustWork { work_id: left_id, cause_digest: left_digest },
            SchedulerEventKind::WorkExhausted { work_id: right_id, cause_digest: right_digest },
        ) => left_id == right_id && left_digest == right_digest,
        (
            SchedulerCommandKind::AbandonDispatch {
                dispatch_id: left_id,
                cause_digest: left_digest,
            },
            SchedulerEventKind::DispatchAbandoned {
                dispatch_id: right_id,
                cause_digest: right_digest,
            },
        ) => left_id == right_id && left_digest == right_digest,
        (SchedulerCommandKind::PauseScheduler, SchedulerEventKind::SchedulerPaused)
        | (SchedulerCommandKind::ResumeScheduler, SchedulerEventKind::SchedulerResumed)
        | (SchedulerCommandKind::DrainScheduler, SchedulerEventKind::SchedulerDrainRequested)
        | (
            SchedulerCommandKind::FinalizeScheduler,
            SchedulerEventKind::SchedulerFinalized { .. },
        ) => true,
        _ => false,
    }
}
