//! Pure command decision and deterministic event replay.

mod apply;

use peritus_types::Sha256Digest;

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};

use super::{
    DebuggerCommand, DebuggerCommandKind, DebuggerEvent, DebuggerEventKind, DebuggerState,
    DebuggerTransition,
};

/// Decides one command without mutating caller-owned prior state.
///
/// # Errors
///
/// Rejects stale fences, terminal-state transitions, illegal lifecycle commands, overflow, or
/// canonical successor-state encoding failure.
pub fn decide(
    prior: Option<&DebuggerState>,
    command: &DebuggerCommand,
) -> Result<DebuggerTransition, DebuggerError> {
    validate_fence(prior, command)?;
    let sequence = command
        .expected_sequence()
        .checked_add(1)
        .ok_or_else(|| invalid("debugger aggregate sequence overflowed"))?;
    let (kind, mut state) = apply::command(prior, command, sequence)?;
    state.refresh_digest()?;
    let event = DebuggerEvent::new(
        command.event_id(),
        command.command_id(),
        command.job_id(),
        sequence,
        command.expected_previous_event(),
        command.prior_state_digest(),
        command.query_digest(),
        command.digest(),
        state.state_digest(),
        kind,
    );
    Ok(DebuggerTransition::new(event, state))
}

/// Applies one checked event during deterministic replay.
///
/// # Errors
///
/// Rejects a noncontiguous event, semantic/digest drift, or an event that does not reproduce its
/// advertised successor state.
pub fn apply_event(
    prior: Option<&DebuggerState>,
    event: &DebuggerEvent,
) -> Result<DebuggerState, DebuggerError> {
    let (expected_sequence, expected_previous, expected_digest) =
        prior.map_or((1, None, Sha256Digest::new([0; 32])), |state| {
            (state.sequence().saturating_add(1), Some(state.last_event_id()), state.state_digest())
        });
    if event.sequence() != expected_sequence
        || event.previous_event() != expected_previous
        || event.prior_state_digest() != expected_digest
    {
        return Err(replay_error("event fence differs from reconstructed state"));
    }
    let command = DebuggerCommand::new(
        event.command_id(),
        event.id(),
        event.job_id(),
        event.sequence() - 1,
        event.previous_event(),
        event.prior_state_digest(),
        event.query_digest(),
        event_to_command(*event.kind(), event.query_digest()),
    )?;
    if command.digest() != event.command_digest() {
        return Err(replay_error("event command digest differs from semantic payload"));
    }
    let transition = decide(prior, &command)?;
    if transition.event() != event {
        return Err(replay_error("event successor differs from deterministic reduction"));
    }
    Ok(transition.into_parts().1)
}

/// Rebuilds complete debugger state from a nonempty contiguous event prefix.
///
/// # Errors
///
/// Rejects an empty sequence or any event that fails complete replay validation.
pub fn replay(events: &[DebuggerEvent]) -> Result<DebuggerState, DebuggerError> {
    let mut state = None;
    for event in events {
        state = Some(apply_event(state.as_ref(), event)?);
    }
    state.ok_or_else(|| replay_error("cannot rebuild debugger state from an empty event sequence"))
}

fn validate_fence(
    prior: Option<&DebuggerState>,
    command: &DebuggerCommand,
) -> Result<(), DebuggerError> {
    let expected_sequence = command.expected_sequence();
    let valid = prior.map_or_else(
        || {
            expected_sequence == 0
                && command.expected_previous_event().is_none()
                && command.prior_state_digest() == Sha256Digest::new([0; 32])
                && (matches!(command.kind(), DebuggerCommandKind::CreateJob { .. }))
        },
        |state| {
            (!state.phase().is_terminal())
                && state.job_id() == command.job_id()
                && state.query_digest() == command.query_digest()
                && state.sequence() == expected_sequence
                && Some(state.last_event_id()) == command.expected_previous_event()
                && state.state_digest() == command.prior_state_digest()
        },
    );
    if valid {
        Ok(())
    } else if prior.is_some_and(|state| state.phase().is_terminal()) {
        Err(DebuggerError::new(
            DebuggerErrorKind::IllegalTransition,
            DebuggerOperation::ApplyTransition,
            DebuggerRecovery::None,
            "terminal debugger state cannot transition",
        ))
    } else {
        Err(DebuggerError::new(
            DebuggerErrorKind::IdempotencyConflict,
            DebuggerOperation::ApplyTransition,
            DebuggerRecovery::ReplayAggregate,
            "command fence differs from exact debugger state",
        ))
    }
}

#[allow(clippy::too_many_lines, reason = "closed event-to-command mapping stays exhaustive")]
const fn event_to_command(
    kind: DebuggerEventKind,
    query_digest: Sha256Digest,
) -> DebuggerCommandKind {
    match kind {
        DebuggerEventKind::JobCreated { revision, limits_digest, model_plan_digest } => {
            DebuggerCommandKind::CreateJob {
                revision,
                query_digest,
                limits_digest,
                model_plan_digest,
            }
        }
        DebuggerEventKind::SelectionRecorded { selection } => {
            DebuggerCommandKind::RecordSelection { selection }
        }
        DebuggerEventKind::DeterministicAnalysisRecorded { analysis_digest, counts } => {
            DebuggerCommandKind::RecordDeterministicAnalysis { analysis_digest, counts }
        }
        DebuggerEventKind::ModelAnalysisRequested {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        } => DebuggerCommandKind::RequestModelAnalysis {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        },
        DebuggerEventKind::ModelAttemptStarted { model_id, attempt, started_at_tick } => {
            DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt, started_at_tick }
        }
        DebuggerEventKind::ModelProposalRecorded {
            model_id,
            attempt,
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        } => DebuggerCommandKind::RecordModelProposal {
            model_id,
            attempt,
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        },
        DebuggerEventKind::ModelFailureRecorded { failure } => {
            DebuggerCommandKind::RecordModelFailure { failure }
        }
        DebuggerEventKind::ModelRetryScheduled { model_id, next_attempt, not_before_tick } => {
            DebuggerCommandKind::ScheduleModelRetry { model_id, next_attempt, not_before_tick }
        }
        DebuggerEventKind::JobCancelled { reason_digest } => {
            DebuggerCommandKind::CancelJob { reason_digest }
        }
        DebuggerEventKind::ReportCompleted { report } => {
            DebuggerCommandKind::CompleteReport { report }
        }
        DebuggerEventKind::PublicationRecorded { publication } => {
            DebuggerCommandKind::RecordPublication { publication }
        }
        DebuggerEventKind::JobFailed { failure } => DebuggerCommandKind::FailJob { failure },
    }
}

fn invalid(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::IllegalTransition,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}

fn replay_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::Replay,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
