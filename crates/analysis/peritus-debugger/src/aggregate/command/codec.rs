//! Canonical semantic command encoding shared by frames and state checkpoints.

use peritus_codec::CanonicalWriter;
use peritus_types::{RevisionTuple, Sha256Digest};

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};

use super::super::{
    AnalysisCounts, ModelAttemptFailure, ModelBudget, ModelRetryPolicy, PublicationRecord,
    ReportRecord,
};
use super::DebuggerCommandKind;

#[allow(clippy::too_many_lines, reason = "closed command schema mapping stays exhaustive")]
pub(in crate::aggregate) fn encode_kind(
    writer: &mut CanonicalWriter,
    kind: &DebuggerCommandKind,
) -> Result<(), DebuggerError> {
    match kind {
        DebuggerCommandKind::CreateJob {
            revision,
            query_digest,
            limits_digest,
            model_plan_digest,
        } => {
            writer.write_u8(1).map_err(codec)?;
            encode_revision(writer, revision)?;
            writer.write_fixed(query_digest.as_bytes()).map_err(codec)?;
            writer.write_fixed(limits_digest.as_bytes()).map_err(codec)?;
            write_optional_digest(writer, *model_plan_digest)
        }
        DebuggerCommandKind::RecordSelection { selection } => {
            writer.write_u8(2).map_err(codec)?;
            writer.write_fixed(selection.id().as_bytes()).map_err(codec)?;
            writer.write_fixed(selection.digest().as_bytes()).map_err(codec)?;
            writer.write_u64(selection.subjects()).map_err(codec)?;
            writer.write_u64(selection.events()).map_err(codec)
        }
        DebuggerCommandKind::RecordDeterministicAnalysis { analysis_digest, counts } => {
            writer.write_u8(3).map_err(codec)?;
            writer.write_fixed(analysis_digest.as_bytes()).map_err(codec)?;
            encode_counts(writer, *counts)
        }
        DebuggerCommandKind::RequestModelAnalysis {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        } => {
            writer.write_u8(4).map_err(codec)?;
            writer.write_fixed(model_id.as_bytes()).map_err(codec)?;
            writer.write_fixed(plan_digest.as_bytes()).map_err(codec)?;
            writer.write_fixed(request_digest.as_bytes()).map_err(codec)?;
            encode_model_budget(writer, *budget)?;
            encode_retry_policy(writer, *retry_policy)
        }
        DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt, started_at_tick } => {
            writer.write_u8(5).map_err(codec)?;
            writer.write_fixed(model_id.as_bytes()).map_err(codec)?;
            writer.write_u16(*attempt).map_err(codec)?;
            writer.write_u64(*started_at_tick).map_err(codec)
        }
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
        } => {
            writer.write_u8(6).map_err(codec)?;
            writer.write_fixed(model_id.as_bytes()).map_err(codec)?;
            writer.write_u16(*attempt).map_err(codec)?;
            writer.write_fixed(proposal_digest.as_bytes()).map_err(codec)?;
            writer.write_fixed(output_digest.as_bytes()).map_err(codec)?;
            writer.write_u64(*output_bytes).map_err(codec)?;
            writer.write_u64(*event_count).map_err(codec)?;
            writer.write_u64(*input_tokens).map_err(codec)?;
            writer.write_u64(*output_tokens).map_err(codec)?;
            writer.write_u64(*total_tokens).map_err(codec)
        }
        DebuggerCommandKind::RecordModelFailure { failure } => {
            writer.write_u8(7).map_err(codec)?;
            encode_model_failure(writer, *failure)
        }
        DebuggerCommandKind::ScheduleModelRetry { model_id, next_attempt, not_before_tick } => {
            writer.write_u8(8).map_err(codec)?;
            writer.write_fixed(model_id.as_bytes()).map_err(codec)?;
            writer.write_u16(*next_attempt).map_err(codec)?;
            writer.write_u64(*not_before_tick).map_err(codec)
        }
        DebuggerCommandKind::CancelJob { reason_digest } => {
            writer.write_u8(9).map_err(codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(codec)
        }
        DebuggerCommandKind::CompleteReport { report } => {
            writer.write_u8(10).map_err(codec)?;
            encode_report(writer, *report)
        }
        DebuggerCommandKind::RecordPublication { publication } => {
            writer.write_u8(11).map_err(codec)?;
            encode_publication(writer, *publication)
        }
        DebuggerCommandKind::FailJob { failure } => {
            writer.write_u8(12).map_err(codec)?;
            writer.write_u8(failure.code().tag()).map_err(codec)?;
            writer.write_fixed(failure.diagnostic_digest().as_bytes()).map_err(codec)
        }
    }
}

pub(in crate::aggregate) fn encode_revision(
    writer: &mut CanonicalWriter,
    revision: &RevisionTuple,
) -> Result<(), DebuggerError> {
    writer.write_fixed(revision.acceptance_spec_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(revision.harness_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(revision.workspace_id().as_bytes()).map_err(codec)?;
    writer.write_u64(revision.workspace_generation().get()).map_err(codec)?;
    writer.write_u64(revision.workspace_revision().get()).map_err(codec)?;
    writer.write_fixed(revision.policy_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(revision.provider_profile_id().as_bytes()).map_err(codec)
}

pub(in crate::aggregate) fn encode_counts(
    writer: &mut CanonicalWriter,
    counts: AnalysisCounts,
) -> Result<(), DebuggerError> {
    writer.write_u64(counts.claims()).map_err(codec)?;
    writer.write_u64(counts.causes()).map_err(codec)?;
    writer.write_u64(counts.patterns()).map_err(codec)
}

pub(in crate::aggregate) fn encode_model_budget(
    writer: &mut CanonicalWriter,
    budget: ModelBudget,
) -> Result<(), DebuggerError> {
    writer.write_u64(budget.max_events()).map_err(codec)?;
    writer.write_u64(budget.max_output_bytes()).map_err(codec)?;
    writer.write_u64(budget.max_input_tokens()).map_err(codec)?;
    writer.write_u64(budget.max_output_tokens()).map_err(codec)?;
    writer.write_u64(budget.max_total_tokens()).map_err(codec)
}

pub(in crate::aggregate) fn encode_retry_policy(
    writer: &mut CanonicalWriter,
    policy: ModelRetryPolicy,
) -> Result<(), DebuggerError> {
    writer.write_u16(policy.max_attempts()).map_err(codec)?;
    writer.write_u64(policy.max_delay_ticks()).map_err(codec)
}

pub(in crate::aggregate) fn encode_model_failure(
    writer: &mut CanonicalWriter,
    failure: ModelAttemptFailure,
) -> Result<(), DebuggerError> {
    writer.write_fixed(failure.model_id().as_bytes()).map_err(codec)?;
    writer.write_u16(failure.attempt()).map_err(codec)?;
    writer.write_u8(failure.code().tag()).map_err(codec)?;
    writer.write_bool(failure.retryable()).map_err(codec)?;
    writer.write_fixed(failure.diagnostic_digest().as_bytes()).map_err(codec)?;
    writer.write_u64(failure.event_count()).map_err(codec)?;
    writer.write_u64(failure.total_tokens()).map_err(codec)
}

pub(in crate::aggregate) fn encode_report(
    writer: &mut CanonicalWriter,
    report: ReportRecord,
) -> Result<(), DebuggerError> {
    writer.write_fixed(report.id().as_bytes()).map_err(codec)?;
    writer.write_fixed(report.digest().as_bytes()).map_err(codec)?;
    writer.write_u64(report.size()).map_err(codec)
}

pub(in crate::aggregate) fn encode_publication(
    writer: &mut CanonicalWriter,
    publication: PublicationRecord,
) -> Result<(), DebuggerError> {
    writer.write_fixed(publication.report_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(publication.artifact_digest().as_bytes()).map_err(codec)?;
    writer.write_u64(publication.artifact_size()).map_err(codec)?;
    writer.write_fixed(publication.evidence_id().as_bytes()).map_err(codec)?;
    writer.write_u64(publication.journal_position()).map_err(codec)
}

fn write_optional_digest(
    writer: &mut CanonicalWriter,
    digest: Option<Sha256Digest>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(digest.is_some()).map_err(codec)?;
    if let Some(value) = digest {
        writer.write_fixed(value.as_bytes()).map_err(codec)?;
    }
    Ok(())
}

fn codec(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Budget,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::CorrectInput,
        error.to_string(),
    )
}
