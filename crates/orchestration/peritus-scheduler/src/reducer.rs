//! Pure deterministic scheduler reduction and exact replay.

mod apply;
mod decision;
mod fences;
mod reconstruction;
mod start;

use std::collections::BTreeSet;

use peritus_types::Sha256Digest;

use crate::{
    SchedulerCommand, SchedulerCommandKind, SchedulerError, SchedulerErrorKind, SchedulerEvent,
    SchedulerState, SchedulerTransition,
};

use apply::apply;
use reconstruction::command_from_event;

/// Starts a scheduler from the only legal genesis command.
///
/// # Errors
/// Rejects non-genesis commands, bad binding, or mismatched run/revision/fences.
pub fn start(command: &SchedulerCommand) -> Result<SchedulerTransition, SchedulerError> {
    let SchedulerCommandKind::StartScheduler { binding } = command.kind() else {
        return Err(illegal("scheduler genesis command is not StartScheduler"));
    };
    binding.validate()?;
    if command.semantics() != binding.semantics()
        || command.run_id() != binding.run_id()
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
    let mut state = start::prepare_genesis(binding, command.event_id(), command.command_id());
    if state.estimated_encoded_bytes() > binding.limits().state_bytes() {
        return Err(crate::error::reject(
            SchedulerErrorKind::LimitExceeded,
            "scheduler genesis exceeds its state-byte bound",
        ));
    }
    let successor = crate::canonical::state_digest(&state);
    let event = start::commit_genesis(command, binding, &mut state, successor);
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
    decision::prepare_cursor(&mut successor, sequence, command.event_id(), command.command_id());
    let successor_digest = crate::canonical::state_digest(&successor);
    let event = decision::commit_event(state, command, &mut successor, kind, successor_digest);
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
    let first_command = command_from_event(first, 0, None);
    let first_transition = start(&first_command)?;
    if first_transition.event() != first {
        return Err(replay_error("scheduler genesis differs from deterministic reduction"));
    }
    let mut state = first_transition.into_state();
    let mut event_ids = BTreeSet::from([first.id()]);
    let mut command_ids = BTreeSet::from([first.command_id()]);
    for event in &events[1..] {
        if event.semantics() != first.semantics() {
            return Err(replay_error("scheduler replay mixes semantic versions"));
        }
        if !event_ids.insert(event.id()) || !command_ids.insert(event.command_id()) {
            return Err(replay_error("scheduler event or command identity is duplicated"));
        }
        let command =
            command_from_event(event, state.sequence().get(), Some(state.last_event_id()));
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
    match fences::classify(state, command) {
        fences::FenceAdmission::Accepted => Ok(()),
        fences::FenceAdmission::Terminal => {
            Err(illegal("scheduler aggregate is terminal and fenced closed"))
        }
        fences::FenceAdmission::HistoryLimit => Err(crate::error::reject(
            SchedulerErrorKind::LimitExceeded,
            "scheduler command history reached the canonical collection limit",
        )),
        fences::FenceAdmission::Stale => Err(crate::error::reject(
            SchedulerErrorKind::StaleFence,
            "scheduler command run, revision, predecessor, digest, identity, or lifecycle differs",
        )),
    }
}

pub fn illegal(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::IllegalTransition, detail)
}

fn replay_error(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::ReplayMismatch, detail)
}
