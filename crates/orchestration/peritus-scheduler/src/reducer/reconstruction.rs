//! Exact causative-command reconstruction used by production event replay.

#[cfg(verus_only)]
mod relation;

use crate::{
    SchedulerCommand, SchedulerCommandKind as Command, SchedulerEvent, SchedulerEventKind as Event,
};
use peritus_types::EventId;
#[cfg(verus_only)]
use relation::{command_matches, payload_matches};
use vstd::prelude::*;

verus! {

fn payload_from_event(event: &Event) -> (command: Command)
    ensures payload_matches(event, &command),
{
    match event {
        Event::SchedulerStarted { binding } => Command::StartScheduler { binding: binding.clone() },
        Event::WorkerRegistered { descriptor } => {
            Command::RegisterWorker { descriptor: descriptor.clone() }
        },
        Event::WorkerAvailable { worker_id } => Command::SetWorkerAvailable { worker_id: *worker_id },
        Event::WorkerDrainRequested { worker_id } => Command::DrainWorker { worker_id: *worker_id },
        Event::WorkerLost { worker_id, .. } => Command::LoseWorker { worker_id: *worker_id },
        Event::WorkerRemoved { worker_id } => Command::RemoveWorker { worker_id: *worker_id },
        Event::WorkAdmitted { spec } => Command::AdmitWork { spec: spec.clone() },
        Event::WorkReserved { reservation } => Command::DispatchNext {
            dispatch_id: reservation.dispatch_id(),
            dispatch_token: reservation.dispatch_token(),
        },
        Event::WorkStartAcknowledged { dispatch_id } => {
            Command::AcknowledgeStart { dispatch_id: *dispatch_id }
        },
        Event::WorkSucceeded { dispatch_id, result_digest } => Command::CompleteWork {
            dispatch_id: *dispatch_id, result_digest: *result_digest,
        },
        Event::WorkFailed { dispatch_id, failure_digest, disposition } => Command::FailWork {
            dispatch_id: *dispatch_id, failure_digest: *failure_digest, disposition: *disposition,
        },
        Event::WorkRetryQueued { work_id } => Command::RetryWork { work_id: *work_id },
        Event::WorkCancelled { work_id, descendants, .. } => {
            if *descendants {
                Command::CancelWorkTree { work_id: *work_id }
            } else {
                Command::CancelWork { work_id: *work_id }
            }
        },
        Event::CancellationAcknowledged { dispatch_id } => {
            Command::AcknowledgeCancellation { dispatch_id: *dispatch_id }
        },
        Event::WorkExhausted { work_id, cause_digest } => Command::ExhaustWork {
            work_id: *work_id, cause_digest: *cause_digest,
        },
        Event::DispatchAbandoned { dispatch_id, cause_digest } => Command::AbandonDispatch {
            dispatch_id: *dispatch_id, cause_digest: *cause_digest,
        },
        Event::SchedulerPaused => Command::PauseScheduler,
        Event::SchedulerResumed => Command::ResumeScheduler,
        Event::SchedulerDrainRequested => Command::DrainScheduler,
        Event::SchedulerFinalized { .. } => Command::FinalizeScheduler,
    }
}

/// Reconstructs inputs; replay separately compares every derived event output.
pub(super) fn command_from_event(
    event: &SchedulerEvent,
    expected_sequence: u64,
    previous: Option<EventId>,
) -> (command: SchedulerCommand)
    ensures command_matches(event, expected_sequence, previous, &command),
{
    let kind = payload_from_event(event.kind());
    SchedulerCommand::from_wire(
        event.semantics(),
        event.command_id(),
        event.id(),
        event.run_id(),
        expected_sequence,
        previous,
        event.prior_state_digest(),
        event.revision(),
        kind,
    )
}

} // verus!
