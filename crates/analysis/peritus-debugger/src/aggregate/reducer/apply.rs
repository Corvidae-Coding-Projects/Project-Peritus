//! Exhaustive command-to-event/state reduction.

mod model;

use peritus_types::Sha256Digest;

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};

use super::super::{
    DebuggerCommand, DebuggerCommandKind, DebuggerEventKind, DebuggerPhase, DebuggerState,
};

#[allow(
    clippy::too_many_lines,
    reason = "the closed lifecycle match keeps every command and legal phase adjacent"
)]
pub(super) fn command(
    prior: Option<&DebuggerState>,
    command: &DebuggerCommand,
    sequence: u64,
) -> Result<(DebuggerEventKind, DebuggerState), DebuggerError> {
    match (prior, command.kind()) {
        (
            None,
            DebuggerCommandKind::CreateJob {
                revision,
                query_digest,
                limits_digest,
                model_plan_digest,
            },
        ) => create_job(
            command,
            sequence,
            *revision,
            *query_digest,
            *limits_digest,
            *model_plan_digest,
        ),
        (None, _) => Err(illegal("first debugger command must create the job")),
        (Some(_), DebuggerCommandKind::CreateJob { .. }) => {
            Err(illegal("debugger job cannot be created twice"))
        }
        (Some(prior), DebuggerCommandKind::RecordSelection { selection }) => {
            require_phase(prior, DebuggerPhase::Created)?;
            let mut state = prior.clone();
            state.selection = Some(*selection);
            state.phase = DebuggerPhase::Selected;
            advance(&mut state, command, sequence);
            Ok((DebuggerEventKind::SelectionRecorded { selection: *selection }, state))
        }
        (
            Some(prior),
            DebuggerCommandKind::RecordDeterministicAnalysis { analysis_digest, counts },
        ) => {
            require_phase(prior, DebuggerPhase::Selected)?;
            let mut state = prior.clone();
            state.deterministic_digest = Some(*analysis_digest);
            state.analysis_counts = Some(*counts);
            state.phase = DebuggerPhase::DeterministicComplete;
            advance(&mut state, command, sequence);
            Ok((
                DebuggerEventKind::DeterministicAnalysisRecorded {
                    analysis_digest: *analysis_digest,
                    counts: *counts,
                },
                state,
            ))
        }
        (
            Some(prior),
            DebuggerCommandKind::RequestModelAnalysis {
                model_id,
                plan_digest,
                request_digest,
                budget,
                retry_policy,
            },
        ) => model::request(
            prior,
            command,
            sequence,
            *model_id,
            *plan_digest,
            *request_digest,
            *budget,
            *retry_policy,
        ),
        (
            Some(prior),
            DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt, started_at_tick },
        ) => model::start(prior, command, sequence, *model_id, *attempt, *started_at_tick),
        (
            Some(prior),
            DebuggerCommandKind::RecordModelProposal {
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
        ) => model::record_proposal(
            prior,
            command,
            sequence,
            *model_id,
            *attempt,
            *proposal_digest,
            *output_digest,
            *output_bytes,
            *event_count,
            *input_tokens,
            *output_tokens,
            *total_tokens,
        ),
        (Some(prior), DebuggerCommandKind::RecordModelFailure { failure }) => {
            model::record_failure(prior, command, sequence, *failure)
        }
        (
            Some(prior),
            DebuggerCommandKind::ScheduleModelRetry { model_id, next_attempt, not_before_tick },
        ) => model::schedule_retry(
            prior,
            command,
            sequence,
            *model_id,
            *next_attempt,
            *not_before_tick,
        ),
        (Some(prior), DebuggerCommandKind::CancelJob { reason_digest }) => {
            let mut state = prior.clone();
            state.phase = DebuggerPhase::Cancelled;
            state.cancellation_reason_digest = Some(*reason_digest);
            advance(&mut state, command, sequence);
            Ok((DebuggerEventKind::JobCancelled { reason_digest: *reason_digest }, state))
        }
        (Some(prior), DebuggerCommandKind::CompleteReport { report }) => {
            if !matches!(
                prior.phase(),
                DebuggerPhase::DeterministicComplete | DebuggerPhase::ModelValidated
            ) {
                return Err(illegal("report completion requires finished deterministic analysis"));
            }
            let mut state = prior.clone();
            state.report = Some(*report);
            state.phase = DebuggerPhase::ReportReady;
            advance(&mut state, command, sequence);
            Ok((DebuggerEventKind::ReportCompleted { report: *report }, state))
        }
        (Some(prior), DebuggerCommandKind::RecordPublication { publication }) => {
            require_phase(prior, DebuggerPhase::ReportReady)?;
            let report =
                prior.report().ok_or_else(|| illegal("report-ready state has no report"))?;
            if publication.report_id() != report.id()
                || publication.artifact_digest() != report.digest()
                || publication.artifact_size() != report.size()
            {
                return Err(conflict("publication differs from the committed report"));
            }
            let mut state = prior.clone();
            state.publication = Some(*publication);
            state.phase = DebuggerPhase::Published;
            advance(&mut state, command, sequence);
            Ok((DebuggerEventKind::PublicationRecorded { publication: *publication }, state))
        }
        (Some(prior), DebuggerCommandKind::FailJob { failure }) => {
            let mut state = prior.clone();
            state.failure = Some(*failure);
            state.phase = DebuggerPhase::Failed;
            advance(&mut state, command, sequence);
            Ok((DebuggerEventKind::JobFailed { failure: *failure }, state))
        }
    }
}

fn create_job(
    command: &DebuggerCommand,
    sequence: u64,
    revision: peritus_types::RevisionTuple,
    query_digest: Sha256Digest,
    limits_digest: Sha256Digest,
    model_plan_digest: Option<Sha256Digest>,
) -> Result<(DebuggerEventKind, DebuggerState), DebuggerError> {
    if command.query_digest() != query_digest {
        return Err(conflict("create payload and command query digests differ"));
    }
    let state = DebuggerState {
        job_id: command.job_id(),
        revision,
        query_digest,
        limits_digest,
        model_plan_digest,
        sequence,
        last_event_id: command.event_id(),
        state_digest: Sha256Digest::new([0; 32]),
        phase: DebuggerPhase::Created,
        selection: None,
        deterministic_digest: None,
        analysis_counts: None,
        model: None,
        model_attempts: Vec::new(),
        report: None,
        publication: None,
        failure: None,
        cancellation_reason_digest: None,
    };
    Ok((DebuggerEventKind::JobCreated { revision, limits_digest, model_plan_digest }, state))
}

pub(super) fn require_phase(
    state: &DebuggerState,
    expected: DebuggerPhase,
) -> Result<(), DebuggerError> {
    if state.phase() == expected {
        Ok(())
    } else {
        Err(illegal("command is illegal in this job phase"))
    }
}

pub(super) const fn advance(state: &mut DebuggerState, command: &DebuggerCommand, sequence: u64) {
    state.sequence = sequence;
    state.last_event_id = command.event_id();
}

pub(super) fn illegal(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::IllegalTransition,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}

pub(super) fn conflict(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::IdempotencyConflict,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
