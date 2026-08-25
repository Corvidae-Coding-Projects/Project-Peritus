//! Pure deterministic collaboration transition and exact replay.

mod apply;

use std::collections::BTreeSet;

use peritus_types::{EventSequence, Sha256Digest};

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use crate::state::mutation;
use crate::{
    CollaborationCommand, CollaborationCommandKind, CollaborationEvent, CollaborationEventKind,
    CollaborationPhase, CollaborationState, CollaborationTransition,
};

/// Starts one collaboration aggregate from the only legal genesis command.
///
/// # Errors
/// Rejects malformed binding, non-genesis fences, or mismatched run/revision.
pub fn start(
    command: &CollaborationCommand,
) -> Result<CollaborationTransition, CollaborationError> {
    let CollaborationCommandKind::Start { binding } = command.kind() else {
        return Err(illegal("genesis command is not Start"));
    };
    binding.validate()?;
    if command.run_id() != binding.run_id()
        || command.revision() != binding.revision()
        || command.expected_sequence() != 0
        || command.expected_previous_event().is_some()
        || command.prior_state_digest() != Sha256Digest::new([0; 32])
    {
        return Err(reject(
            CollaborationErrorKind::BindingMismatch,
            "genesis command differs from exact collaboration binding or fences",
        ));
    }
    let sequence = EventSequence::first();
    let mut state = CollaborationState::genesis(
        binding.clone(),
        sequence,
        command.event_id(),
        command.command_id(),
    );
    ensure_state_bound(&state)?;
    let digest = crate::canonical::state_digest(&state);
    mutation::set_state_digest(&mut state, digest);
    let event = CollaborationEvent::from_wire(
        command.event_id(),
        command.command_id(),
        sequence,
        None,
        command.run_id(),
        command.revision(),
        Sha256Digest::new([0; 32]),
        digest,
        CollaborationEventKind::Started { binding: binding.clone() },
    );
    Ok(CollaborationTransition::new(event, state))
}

/// Applies one fenced command to cloned state without performing external effects.
///
/// # Errors
/// Rejects stale fences, illegal ownership/lifecycle, invalid causality, join violations, and
/// configured limit exhaustion without changing the input state.
pub fn decide(
    state: &CollaborationState,
    command: &CollaborationCommand,
) -> Result<CollaborationTransition, CollaborationError> {
    validate_fences(state, command)?;
    if estimated_command_bytes(command.kind()) > state.limits().command_bytes() {
        return Err(reject(
            CollaborationErrorKind::LimitExceeded,
            "collaboration command exceeds its immutable byte limit",
        ));
    }
    let sequence = state.sequence().checked_next().map_err(|_| {
        reject(CollaborationErrorKind::LimitExceeded, "collaboration event sequence overflowed")
    })?;
    let mut successor = state.clone();
    let kind = apply::apply(&mut successor, command.kind())?;
    ensure_state_bound(&successor)?;
    mutation::advance_cursor(&mut successor, sequence, command.event_id(), command.command_id());
    let successor_digest = crate::canonical::state_digest(&successor);
    mutation::set_state_digest(&mut successor, successor_digest);
    let event = CollaborationEvent::from_wire(
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
    Ok(CollaborationTransition::new(event, successor))
}

/// Reconstructs exact state from canonical events.
///
/// # Errors
/// Rejects empty, duplicated, reordered, stale, tampered, or semantically illegal streams.
pub fn replay(events: &[CollaborationEvent]) -> Result<CollaborationState, CollaborationError> {
    let first = events.first().ok_or_else(|| {
        reject(CollaborationErrorKind::ReplayMismatch, "collaboration replay is empty")
    })?;
    let first_command = command_from_event(first, 0, None)?;
    let transition = start(&first_command)?;
    if transition.event() != first {
        return Err(replay_error("collaboration genesis differs from deterministic reduction"));
    }
    let mut state = transition.into_state();
    let mut event_ids = BTreeSet::from([first.id()]);
    let mut command_ids = BTreeSet::from([first.command_id()]);
    for event in &events[1..] {
        if !event_ids.insert(event.id()) || !command_ids.insert(event.command_id()) {
            return Err(replay_error("collaboration event or command identity is duplicated"));
        }
        let command =
            command_from_event(event, state.sequence().get(), Some(state.last_event_id()))?;
        let transition = decide(&state, &command)?;
        if transition.event() != event {
            return Err(replay_error("collaboration event differs from deterministic reduction"));
        }
        state = transition.into_state();
    }
    Ok(state)
}

fn validate_fences(
    state: &CollaborationState,
    command: &CollaborationCommand,
) -> Result<(), CollaborationError> {
    if state.phase() == CollaborationPhase::Terminal {
        return Err(illegal("collaboration aggregate is terminal and fenced closed"));
    }
    if state.run_id() != command.run_id()
        || state.binding().revision() != command.revision()
        || state.sequence().get() != command.expected_sequence()
        || command.expected_previous_event() != Some(state.last_event_id())
        || command.prior_state_digest() != state.state_digest()
        || state.used_commands().contains(&command.command_id())
        || matches!(command.kind(), CollaborationCommandKind::Start { .. })
    {
        return Err(reject(
            CollaborationErrorKind::StaleFence,
            "command run, revision, predecessor, state, identity, or lifecycle fence differs",
        ));
    }
    Ok(())
}

fn command_from_event(
    event: &CollaborationEvent,
    expected_sequence: u64,
    previous: Option<peritus_types::EventId>,
) -> Result<CollaborationCommand, CollaborationError> {
    let kind = match event.kind() {
        CollaborationEventKind::Started { binding } => {
            CollaborationCommandKind::Start { binding: binding.clone() }
        }
        CollaborationEventKind::DelegationOffered { offered_by, assignment } => {
            CollaborationCommandKind::OfferDelegation {
                offered_by: *offered_by,
                assignment: assignment.clone(),
            }
        }
        CollaborationEventKind::DelegationAccepted { task_id, accepted_by } => {
            CollaborationCommandKind::AcceptDelegation {
                task_id: *task_id,
                accepted_by: *accepted_by,
            }
        }
        CollaborationEventKind::DelegationRejected { task_id, rejected_by, reason_digest } => {
            CollaborationCommandKind::RejectDelegation {
                task_id: *task_id,
                rejected_by: *rejected_by,
                reason_digest: *reason_digest,
            }
        }
        CollaborationEventKind::TaskActivated { task_id, observation } => {
            CollaborationCommandKind::ActivateTask { task_id: *task_id, observation: *observation }
        }
        CollaborationEventKind::MessageSent { message } => {
            CollaborationCommandKind::SendMessage { message: message.clone() }
        }
        CollaborationEventKind::MessageAcknowledged { message_id, receiver } => {
            CollaborationCommandKind::AcknowledgeMessage {
                message_id: *message_id,
                receiver: *receiver,
            }
        }
        CollaborationEventKind::TaskCompleted { task_id, completed_by, terminal } => {
            CollaborationCommandKind::CompleteTask {
                task_id: *task_id,
                completed_by: *completed_by,
                terminal: *terminal,
            }
        }
        CollaborationEventKind::TaskAbandoned { task_id, abandoned_by, reason_digest } => {
            CollaborationCommandKind::AbandonTask {
                task_id: *task_id,
                abandoned_by: *abandoned_by,
                reason_digest: *reason_digest,
            }
        }
        CollaborationEventKind::CancellationPropagated {
            task_id,
            requested_by,
            reason_digest,
            ..
        } => CollaborationCommandKind::CancelTask {
            task_id: *task_id,
            requested_by: *requested_by,
            reason_digest: *reason_digest,
        },
        CollaborationEventKind::CancellationAcknowledged { task_id, owner } => {
            CollaborationCommandKind::AcknowledgeCancellation { task_id: *task_id, owner: *owner }
        }
        CollaborationEventKind::Paused { requested_by } => {
            CollaborationCommandKind::Pause { requested_by: *requested_by }
        }
        CollaborationEventKind::Resumed { requested_by } => {
            CollaborationCommandKind::Resume { requested_by: *requested_by }
        }
        CollaborationEventKind::Finalized => CollaborationCommandKind::Finalize,
    };
    CollaborationCommand::new(
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

fn ensure_state_bound(state: &CollaborationState) -> Result<(), CollaborationError> {
    if state.estimated_encoded_bytes() > state.limits().state_bytes() {
        Err(reject(
            CollaborationErrorKind::LimitExceeded,
            "successor state exceeds its immutable byte limit",
        ))
    } else {
        Ok(())
    }
}

fn estimated_command_bytes(kind: &CollaborationCommandKind) -> u64 {
    match kind {
        CollaborationCommandKind::SendMessage { message } => {
            u64::from(message.payload_bytes()).saturating_add(768)
        }
        CollaborationCommandKind::CancelTask { .. } => 512,
        CollaborationCommandKind::Start { .. } => 1_024,
        _ => 768,
    }
}

pub fn illegal(detail: &'static str) -> CollaborationError {
    reject(CollaborationErrorKind::IllegalTransition, detail)
}

fn replay_error(detail: &'static str) -> CollaborationError {
    reject(CollaborationErrorKind::ReplayMismatch, detail)
}
