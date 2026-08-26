//! Stable C0 identities and cross-record transition validation.

use peritus_journal::{AggregateId, AggregateKey, AggregateKind};

use crate::{
    DebuggerCommand, DebuggerCommandKind, DebuggerError, DebuggerErrorKind, DebuggerEventKind,
    DebuggerJobId, DebuggerOperation, DebuggerRecovery, DebuggerTransition,
};

/// Journal-owned namespace for complete E2 debugger checkpoints.
pub const DEBUGGER_STATE_NAMESPACE: u16 = 0xE201;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.debugger.state-key.v1\0";

/// Derives the dedicated C0 debugger aggregate identity.
///
/// # Errors
/// Rejects a debugger identity C0 cannot represent.
pub fn debugger_aggregate_key(job_id: DebuggerJobId) -> Result<AggregateKey, DebuggerError> {
    let id = AggregateId::new(*job_id.as_bytes()).map_err(journal)?;
    Ok(AggregateKey::new(AggregateKind::Debugger, id))
}

/// Derives the domain-separated complete-checkpoint key.
#[must_use]
pub fn debugger_state_key(job_id: DebuggerJobId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + job_id.as_bytes().len());
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(job_id.as_bytes());
    key
}

#[allow(
    clippy::suspicious_operation_groupings,
    reason = "the binding deliberately compares analogous fields across command, event, and state types"
)]
pub(super) fn validate(
    command: &DebuggerCommand,
    transition: &DebuggerTransition,
) -> Result<(), DebuggerError> {
    let event = transition.event();
    let state = transition.state();
    let reserved_event_id = command.event_id();
    let accepted_event_id = event.id();
    let mismatch = reserved_event_id != accepted_event_id
        || command.command_id() != event.command_id()
        || command.job_id() != event.job_id()
        || command.job_id() != state.job_id()
        || command.expected_previous_event() != event.previous_event()
        || command.expected_sequence().checked_add(1) != Some(event.sequence())
        || command.prior_state_digest() != event.prior_state_digest()
        || command.query_digest() != event.query_digest()
        || command.digest() != event.command_digest()
        || event.successor_state_digest() != state.state_digest()
        || event.sequence() != state.sequence()
        || event.id() != state.last_event_id()
        || !semantic_match(command.kind(), event.kind());
    if mismatch {
        return Err(binding("command, accepted event, and complete successor checkpoint differ"));
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    clippy::match_same_arms,
    reason = "every closed command/event pair remains explicit for durable semantic binding review"
)]
fn semantic_match(command: &DebuggerCommandKind, event: &DebuggerEventKind) -> bool {
    match (command, event) {
        (
            DebuggerCommandKind::CreateJob {
                revision: left_revision,
                limits_digest: left_limits,
                model_plan_digest: left_plan,
                ..
            },
            DebuggerEventKind::JobCreated {
                revision: right_revision,
                limits_digest: right_limits,
                model_plan_digest: right_plan,
            },
        ) => {
            left_revision == right_revision
                && left_limits == right_limits
                && left_plan == right_plan
        }
        (
            DebuggerCommandKind::RecordSelection { selection: left },
            DebuggerEventKind::SelectionRecorded { selection: right },
        ) => left == right,
        (
            DebuggerCommandKind::RecordDeterministicAnalysis {
                analysis_digest: left_digest,
                counts: left_counts,
            },
            DebuggerEventKind::DeterministicAnalysisRecorded {
                analysis_digest: right_digest,
                counts: right_counts,
            },
        ) => left_digest == right_digest && left_counts == right_counts,
        (
            DebuggerCommandKind::RequestModelAnalysis {
                model_id: left_id,
                plan_digest: left_plan,
                request_digest: left_request,
                budget: left_budget,
                retry_policy: left_retry,
            },
            DebuggerEventKind::ModelAnalysisRequested {
                model_id: right_id,
                plan_digest: right_plan,
                request_digest: right_request,
                budget: right_budget,
                retry_policy: right_retry,
            },
        ) => {
            left_id == right_id
                && left_plan == right_plan
                && left_request == right_request
                && left_budget == right_budget
                && left_retry == right_retry
        }
        (
            DebuggerCommandKind::MarkModelAttemptStarted {
                model_id: left_id,
                attempt: left_attempt,
                started_at_tick: left_tick,
            },
            DebuggerEventKind::ModelAttemptStarted {
                model_id: right_id,
                attempt: right_attempt,
                started_at_tick: right_tick,
            },
        ) => left_id == right_id && left_attempt == right_attempt && left_tick == right_tick,
        (
            DebuggerCommandKind::RecordModelProposal {
                model_id: left_id,
                attempt: left_attempt,
                proposal_digest: left_proposal,
                output_digest: left_output,
                output_bytes: left_bytes,
                event_count: left_events,
                input_tokens: left_input,
                output_tokens: left_output_tokens,
                total_tokens: left_total,
            },
            DebuggerEventKind::ModelProposalRecorded {
                model_id: right_id,
                attempt: right_attempt,
                proposal_digest: right_proposal,
                output_digest: right_output,
                output_bytes: right_bytes,
                event_count: right_events,
                input_tokens: right_input,
                output_tokens: right_output_tokens,
                total_tokens: right_total,
            },
        ) => {
            (left_id, left_attempt, left_proposal, left_output, left_bytes, left_events)
                == (
                    right_id,
                    right_attempt,
                    right_proposal,
                    right_output,
                    right_bytes,
                    right_events,
                )
                && (left_input, left_output_tokens, left_total)
                    == (right_input, right_output_tokens, right_total)
        }
        (
            DebuggerCommandKind::RecordModelFailure { failure: left },
            DebuggerEventKind::ModelFailureRecorded { failure: right },
        ) => left == right,
        (
            DebuggerCommandKind::ScheduleModelRetry {
                model_id: left_id,
                next_attempt: left_attempt,
                not_before_tick: left_tick,
            },
            DebuggerEventKind::ModelRetryScheduled {
                model_id: right_id,
                next_attempt: right_attempt,
                not_before_tick: right_tick,
            },
        ) => left_id == right_id && left_attempt == right_attempt && left_tick == right_tick,
        (
            DebuggerCommandKind::CancelJob { reason_digest: left },
            DebuggerEventKind::JobCancelled { reason_digest: right },
        ) => left == right,
        (
            DebuggerCommandKind::CompleteReport { report: left },
            DebuggerEventKind::ReportCompleted { report: right },
        ) => left == right,
        (
            DebuggerCommandKind::RecordPublication { publication: left },
            DebuggerEventKind::PublicationRecorded { publication: right },
        ) => left == right,
        (
            DebuggerCommandKind::FailJob { failure: left },
            DebuggerEventKind::JobFailed { failure: right },
        ) => left == right,
        _ => false,
    }
}

pub(super) fn binding(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Binding,
        DebuggerOperation::CommitTransition,
        DebuggerRecovery::Quarantine,
        detail,
    )
}

pub(super) fn journal(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Journal,
        DebuggerOperation::CommitTransition,
        DebuggerRecovery::ReplayAggregate,
        error.to_string(),
    )
}
