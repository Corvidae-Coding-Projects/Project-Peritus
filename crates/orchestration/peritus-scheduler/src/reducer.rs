//! Pure deterministic scheduler reduction and exact replay.

mod apply;

use std::collections::BTreeSet;

use peritus_types::{EventSequence, Sha256Digest};

use crate::{
    SchedulerCommand, SchedulerCommandKind, SchedulerError, SchedulerErrorKind, SchedulerEvent,
    SchedulerEventKind, SchedulerPhase, SchedulerState, SchedulerTransition,
};

use apply::apply;

/// Starts a scheduler from the only legal genesis command.
///
/// # Errors
/// Rejects non-genesis commands, bad binding, or mismatched run/revision/fences.
pub fn start(command: &SchedulerCommand) -> Result<SchedulerTransition, SchedulerError> {
    let SchedulerCommandKind::StartScheduler { binding } = command.kind() else {
        return Err(illegal("scheduler genesis command is not StartScheduler"));
    };
    binding.validate()?;
    if command.run_id() != binding.run_id()
        || command.revision() != binding.revision()
        || command.expected_sequence() != 0
        || command.expected_previous_event().is_some()
        || command.prior_state_digest() != Sha256Digest::new([0; 32])
    {
        return Err(crate::error::reject(
            SchedulerErrorKind::BindingMismatch,
            "scheduler genesis differs from its exact binding or fences",
        ));
    }
    let mut state =
        SchedulerState::genesis(binding.clone(), command.event_id(), command.command_id());
    if state.estimated_encoded_bytes() > binding.limits().state_bytes() {
        return Err(crate::error::reject(
            SchedulerErrorKind::LimitExceeded,
            "scheduler genesis exceeds its state-byte bound",
        ));
    }
    let successor = crate::canonical::state_digest(&state);
    crate::state::mutation::set_state_digest(&mut state, successor);
    let event = SchedulerEvent::from_wire(
        command.event_id(),
        command.command_id(),
        EventSequence::first(),
        None,
        command.run_id(),
        command.revision(),
        Sha256Digest::new([0; 32]),
        successor,
        SchedulerEventKind::SchedulerStarted { binding: binding.clone() },
    );
    Ok(SchedulerTransition::new(event, state))
}

/// Applies one fenced command to cloned state without performing effects.
///
/// # Errors
/// Rejects stale fences, illegal lifecycle changes, capacity conflicts, invalid ownership, or
/// bounded-state exhaustion without changing the supplied state.
pub fn decide(
    state: &SchedulerState,
    command: &SchedulerCommand,
) -> Result<SchedulerTransition, SchedulerError> {
    validate_fences(state, command)?;
    let sequence = state.sequence().checked_next().map_err(|_| {
        crate::error::reject(
            SchedulerErrorKind::LimitExceeded,
            "scheduler event sequence overflowed",
        )
    })?;
    let mut successor = state.clone();
    let kind = apply(&mut successor, command.kind())?;
    crate::state::mutation::refresh(&mut successor);
    if successor.estimated_encoded_bytes() > successor.binding().limits().state_bytes() {
        return Err(crate::error::reject(
            SchedulerErrorKind::LimitExceeded,
            "scheduler successor exceeds its immutable state-byte bound",
        ));
    }
    crate::state::mutation::advance_cursor(
        &mut successor,
        sequence,
        command.event_id(),
        command.command_id(),
    );
    let successor_digest = crate::canonical::state_digest(&successor);
    crate::state::mutation::set_state_digest(&mut successor, successor_digest);
    let event = SchedulerEvent::from_wire(
        command.event_id(),
        command.command_id(),
        sequence,
        Some(state.last_event_id()),
        command.run_id(),
        command.revision(),
        state.state_digest(),
        successor_digest,
        kind,
    );
    Ok(SchedulerTransition::new(event, successor))
}

/// Reconstructs exact state from canonical events.
///
/// # Errors
/// Rejects empty, duplicated, reordered, stale, tampered, or semantically illegal streams.
pub fn replay(events: &[SchedulerEvent]) -> Result<SchedulerState, SchedulerError> {
    let first = events.first().ok_or_else(|| {
        crate::error::reject(SchedulerErrorKind::ReplayMismatch, "scheduler replay is empty")
    })?;
    let first_command = command_from_event(first, 0, None)?;
    let first_transition = start(&first_command)?;
    if first_transition.event() != first {
        return Err(replay_error("scheduler genesis differs from deterministic reduction"));
    }
    let mut state = first_transition.into_state();
    let mut event_ids = BTreeSet::from([first.id()]);
    let mut command_ids = BTreeSet::from([first.command_id()]);
    for event in &events[1..] {
        if !event_ids.insert(event.id()) || !command_ids.insert(event.command_id()) {
            return Err(replay_error("scheduler event or command identity is duplicated"));
        }
        let command =
            command_from_event(event, state.sequence().get(), Some(state.last_event_id()))?;
        let transition = decide(&state, &command)?;
        if transition.event() != event {
            return Err(replay_error("scheduler event differs from deterministic reduction"));
        }
        state = transition.into_state();
    }
    Ok(state)
}

fn validate_fences(
    state: &SchedulerState,
    command: &SchedulerCommand,
) -> Result<(), SchedulerError> {
    if state.phase() == SchedulerPhase::Terminal {
        return Err(illegal("scheduler aggregate is terminal and fenced closed"));
    }
    if state.used_commands().len() >= 65_535 {
        return Err(crate::error::reject(
            SchedulerErrorKind::LimitExceeded,
            "scheduler command history reached the canonical collection limit",
        ));
    }
    if state.run_id() != command.run_id()
        || state.binding().revision() != command.revision()
        || state.sequence().get() != command.expected_sequence()
        || command.expected_previous_event() != Some(state.last_event_id())
        || command.prior_state_digest() != state.state_digest()
        || state.used_commands().contains(&command.command_id())
        || matches!(command.kind(), SchedulerCommandKind::StartScheduler { .. })
    {
        return Err(crate::error::reject(
            SchedulerErrorKind::StaleFence,
            "scheduler command run, revision, predecessor, digest, identity, or lifecycle differs",
        ));
    }
    Ok(())
}

fn command_from_event(
    event: &SchedulerEvent,
    expected_sequence: u64,
    previous: Option<peritus_types::EventId>,
) -> Result<SchedulerCommand, SchedulerError> {
    let kind = match event.kind() {
        SchedulerEventKind::SchedulerStarted { binding } => {
            SchedulerCommandKind::StartScheduler { binding: binding.clone() }
        }
        SchedulerEventKind::WorkerRegistered { descriptor } => {
            SchedulerCommandKind::RegisterWorker { descriptor: descriptor.clone() }
        }
        SchedulerEventKind::WorkerAvailable { worker_id } => {
            SchedulerCommandKind::SetWorkerAvailable { worker_id: *worker_id }
        }
        SchedulerEventKind::WorkerDrainRequested { worker_id } => {
            SchedulerCommandKind::DrainWorker { worker_id: *worker_id }
        }
        SchedulerEventKind::WorkerLost { worker_id, .. } => {
            SchedulerCommandKind::LoseWorker { worker_id: *worker_id }
        }
        SchedulerEventKind::WorkerRemoved { worker_id } => {
            SchedulerCommandKind::RemoveWorker { worker_id: *worker_id }
        }
        SchedulerEventKind::WorkAdmitted { spec } => {
            SchedulerCommandKind::AdmitWork { spec: spec.clone() }
        }
        SchedulerEventKind::WorkReserved { reservation } => SchedulerCommandKind::DispatchNext {
            dispatch_id: reservation.dispatch_id(),
            dispatch_token: reservation.dispatch_token(),
        },
        SchedulerEventKind::WorkStartAcknowledged { dispatch_id } => {
            SchedulerCommandKind::AcknowledgeStart { dispatch_id: *dispatch_id }
        }
        SchedulerEventKind::WorkSucceeded { dispatch_id, result_digest } => {
            SchedulerCommandKind::CompleteWork {
                dispatch_id: *dispatch_id,
                result_digest: *result_digest,
            }
        }
        SchedulerEventKind::WorkFailed { dispatch_id, failure_digest, disposition } => {
            SchedulerCommandKind::FailWork {
                dispatch_id: *dispatch_id,
                failure_digest: *failure_digest,
                disposition: *disposition,
            }
        }
        SchedulerEventKind::WorkRetryQueued { work_id } => {
            SchedulerCommandKind::RetryWork { work_id: *work_id }
        }
        SchedulerEventKind::WorkCancelled { work_id, descendants, .. } => {
            if *descendants {
                SchedulerCommandKind::CancelWorkTree { work_id: *work_id }
            } else {
                SchedulerCommandKind::CancelWork { work_id: *work_id }
            }
        }
        SchedulerEventKind::CancellationAcknowledged { dispatch_id } => {
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: *dispatch_id }
        }
        SchedulerEventKind::WorkExhausted { work_id, cause_digest } => {
            SchedulerCommandKind::ExhaustWork { work_id: *work_id, cause_digest: *cause_digest }
        }
        SchedulerEventKind::DispatchAbandoned { dispatch_id, cause_digest } => {
            SchedulerCommandKind::AbandonDispatch {
                dispatch_id: *dispatch_id,
                cause_digest: *cause_digest,
            }
        }
        SchedulerEventKind::SchedulerPaused => SchedulerCommandKind::PauseScheduler,
        SchedulerEventKind::SchedulerResumed => SchedulerCommandKind::ResumeScheduler,
        SchedulerEventKind::SchedulerDrainRequested => SchedulerCommandKind::DrainScheduler,
        SchedulerEventKind::SchedulerFinalized { .. } => SchedulerCommandKind::FinalizeScheduler,
    };
    SchedulerCommand::new(
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

pub fn illegal(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::IllegalTransition, detail)
}

fn replay_error(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::ReplayMismatch, detail)
}
