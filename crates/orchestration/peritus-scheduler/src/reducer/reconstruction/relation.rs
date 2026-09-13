//! Declarative correspondence between accepted facts and their causative inputs.

use crate::{
    SchedulerBinding, SchedulerCommand, SchedulerCommandKind as Command, SchedulerEvent,
    SchedulerEventKind as Event, WorkSpec, WorkerDescriptor,
};
use peritus_types::EventId;
use vstd::prelude::*;

verus! {

/// Derived outputs are recomputed by reduction and are not command authority.
pub open spec fn payload_matches(event: &Event, command: &Command) -> bool {
    match (event, command) {
        (Event::SchedulerStarted { binding: source }, Command::StartScheduler { binding: target }) =>
            SchedulerBinding::clone_equivalent(source, target),
        (Event::WorkerRegistered { descriptor: source }, Command::RegisterWorker { descriptor: target }) =>
            WorkerDescriptor::clone_equivalent(source, target),
        (Event::WorkerAvailable { worker_id: source }, Command::SetWorkerAvailable { worker_id: target })
        | (Event::WorkerDrainRequested { worker_id: source }, Command::DrainWorker { worker_id: target })
        | (Event::WorkerLost { worker_id: source, .. }, Command::LoseWorker { worker_id: target })
        | (Event::WorkerRemoved { worker_id: source }, Command::RemoveWorker { worker_id: target }) =>
            source == target,
        (Event::WorkAdmitted { spec: source }, Command::AdmitWork { spec: target }) =>
            WorkSpec::clone_equivalent(source, target),
        (Event::WorkReserved { reservation }, Command::DispatchNext { dispatch_id, dispatch_token }) => {
            &&& *dispatch_id == reservation.spec_dispatch_id()
            &&& *dispatch_token == reservation.spec_dispatch_token()
        },
        (Event::WorkStartAcknowledged { dispatch_id: source }, Command::AcknowledgeStart { dispatch_id: target })
        | (Event::CancellationAcknowledged { dispatch_id: source }, Command::AcknowledgeCancellation { dispatch_id: target }) =>
            source == target,
        (
            Event::WorkSucceeded { dispatch_id: source, result_digest: source_digest },
            Command::CompleteWork { dispatch_id: target, result_digest: target_digest },
        ) => source == target && source_digest == target_digest,
        (
            Event::WorkFailed { dispatch_id: source, failure_digest: source_digest, disposition: source_disposition },
            Command::FailWork { dispatch_id: target, failure_digest: target_digest, disposition: target_disposition },
        ) => source == target && source_digest == target_digest && source_disposition == target_disposition,
        (Event::WorkRetryQueued { work_id: source }, Command::RetryWork { work_id: target }) =>
            source == target,
        (Event::WorkCancelled { work_id: source, descendants, .. }, Command::CancelWork { work_id: target }) =>
            source == target && !*descendants,
        (Event::WorkCancelled { work_id: source, descendants, .. }, Command::CancelWorkTree { work_id: target }) =>
            source == target && *descendants,
        (
            Event::WorkExhausted { work_id: source, cause_digest: source_digest },
            Command::ExhaustWork { work_id: target, cause_digest: target_digest },
        ) => source == target && source_digest == target_digest,
        (
            Event::DispatchAbandoned { dispatch_id: source, cause_digest: source_digest },
            Command::AbandonDispatch { dispatch_id: target, cause_digest: target_digest },
        ) => source == target && source_digest == target_digest,
        (Event::SchedulerPaused, Command::PauseScheduler)
        | (Event::SchedulerResumed, Command::ResumeScheduler)
        | (Event::SchedulerDrainRequested, Command::DrainScheduler)
        | (Event::SchedulerFinalized { .. }, Command::FinalizeScheduler) => true,
        _ => false,
    }
}

/// Exact fields copied from the event plus the independent current-state cursor inputs.
pub open spec fn command_matches(
    event: &SchedulerEvent,
    expected_sequence: u64,
    previous: Option<EventId>,
    command: &SchedulerCommand,
) -> bool {
    &&& command.spec_semantics() == event.spec_semantics()
    &&& command.spec_command_id() == event.spec_command_id()
    &&& command.spec_event_id() == event.spec_id()
    &&& command.spec_run_id() == event.spec_run_id()
    &&& command.spec_expected_sequence() == expected_sequence
    &&& command.spec_expected_previous_event() == previous
    &&& command.spec_prior_state_digest() == event.spec_prior_state_digest()
    &&& command.spec_revision() == event.spec_revision()
    &&& payload_matches(event.spec_kind(), command.spec_kind())
}

} // verus!
