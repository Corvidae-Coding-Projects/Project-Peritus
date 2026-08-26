//! Strict schema-v1 command/event semantic payload codec.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, WorkspaceId,
};

use crate::{
    AnalysisCounts, DebuggerCommandKind, DebuggerError, DebuggerErrorKind, DebuggerOperation,
    DebuggerRecovery, JobFailure, JobFailureCode, ModelAttemptFailure, ModelAttemptFailureCode,
    ModelBudget, ModelRetryPolicy, PublicationRecord, ReportRecord, SelectionRecord,
};

pub(super) fn encode(kind: &DebuggerCommandKind) -> Result<Vec<u8>, DebuggerError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    crate::aggregate::encode_kind(&mut writer, kind)?;
    Ok(writer.into_bytes())
}

pub(super) fn decode(bytes: &[u8]) -> Result<DebuggerCommandKind, DebuggerError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let tag = reader.read_u8().map_err(codec)?;
    let kind = match tag {
        1 => DebuggerCommandKind::CreateJob {
            revision: revision(&mut reader)?,
            query_digest: digest(&mut reader)?,
            limits_digest: digest(&mut reader)?,
            model_plan_digest: optional_digest(&mut reader)?,
        },
        2 => DebuggerCommandKind::RecordSelection {
            selection: SelectionRecord::new(
                crate::SelectionManifestId::new(reader.read_fixed().map_err(codec)?)?,
                digest(&mut reader)?,
                reader.read_u64().map_err(codec)?,
                reader.read_u64().map_err(codec)?,
            )?,
        },
        3 => DebuggerCommandKind::RecordDeterministicAnalysis {
            analysis_digest: digest(&mut reader)?,
            counts: counts(&mut reader)?,
        },
        4 => DebuggerCommandKind::RequestModelAnalysis {
            model_id: crate::ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?,
            plan_digest: digest(&mut reader)?,
            request_digest: digest(&mut reader)?,
            budget: model_budget(&mut reader)?,
            retry_policy: retry_policy(&mut reader)?,
        },
        5 => DebuggerCommandKind::MarkModelAttemptStarted {
            model_id: crate::ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?,
            attempt: reader.read_u16().map_err(codec)?,
            started_at_tick: reader.read_u64().map_err(codec)?,
        },
        6 => DebuggerCommandKind::RecordModelProposal {
            model_id: crate::ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?,
            attempt: reader.read_u16().map_err(codec)?,
            proposal_digest: digest(&mut reader)?,
            output_digest: digest(&mut reader)?,
            output_bytes: reader.read_u64().map_err(codec)?,
            event_count: reader.read_u64().map_err(codec)?,
            input_tokens: reader.read_u64().map_err(codec)?,
            output_tokens: reader.read_u64().map_err(codec)?,
            total_tokens: reader.read_u64().map_err(codec)?,
        },
        7 => DebuggerCommandKind::RecordModelFailure { failure: model_failure(&mut reader)? },
        8 => DebuggerCommandKind::ScheduleModelRetry {
            model_id: crate::ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?,
            next_attempt: reader.read_u16().map_err(codec)?,
            not_before_tick: reader.read_u64().map_err(codec)?,
        },
        9 => DebuggerCommandKind::CancelJob { reason_digest: digest(&mut reader)? },
        10 => DebuggerCommandKind::CompleteReport { report: report(&mut reader)? },
        11 => DebuggerCommandKind::RecordPublication { publication: publication(&mut reader)? },
        12 => DebuggerCommandKind::FailJob {
            failure: JobFailure::new(
                JobFailureCode::from_tag(reader.read_u8().map_err(codec)?)?,
                digest(&mut reader)?,
            ),
        },
        _ => return Err(protocol("unknown debugger semantic tag")),
    };
    reader.finish().map_err(codec)?;
    Ok(kind)
}

fn revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, DebuggerError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid acceptance identity"))?,
        HarnessId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid harness identity"))?,
        WorkspaceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid workspace identity"))?,
        Generation::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| protocol("invalid workspace generation"))?,
        RevisionNumber::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| protocol("invalid workspace revision"))?,
        PolicyId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid policy identity"))?,
        ProviderProfileId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid provider-profile identity"))?,
    ))
}

fn counts(reader: &mut CanonicalReader<'_>) -> Result<AnalysisCounts, DebuggerError> {
    Ok(AnalysisCounts::new(
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    ))
}

fn model_budget(reader: &mut CanonicalReader<'_>) -> Result<ModelBudget, DebuggerError> {
    ModelBudget::new(
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )
}

fn retry_policy(reader: &mut CanonicalReader<'_>) -> Result<ModelRetryPolicy, DebuggerError> {
    ModelRetryPolicy::new(reader.read_u16().map_err(codec)?, reader.read_u64().map_err(codec)?)
}

fn model_failure(reader: &mut CanonicalReader<'_>) -> Result<ModelAttemptFailure, DebuggerError> {
    ModelAttemptFailure::new(
        crate::ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?,
        reader.read_u16().map_err(codec)?,
        ModelAttemptFailureCode::from_tag(reader.read_u8().map_err(codec)?)?,
        reader.read_bool().map_err(codec)?,
        digest(reader)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )
}

fn report(reader: &mut CanonicalReader<'_>) -> Result<ReportRecord, DebuggerError> {
    ReportRecord::new(
        crate::ReportId::new(reader.read_fixed().map_err(codec)?)?,
        digest(reader)?,
        reader.read_u64().map_err(codec)?,
    )
}

fn publication(reader: &mut CanonicalReader<'_>) -> Result<PublicationRecord, DebuggerError> {
    PublicationRecord::new(
        crate::ReportId::new(reader.read_fixed().map_err(codec)?)?,
        digest(reader)?,
        reader.read_u64().map_err(codec)?,
        peritus_types::EvidenceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid evidence identity"))?,
        reader.read_u64().map_err(codec)?,
    )
}

fn optional_digest(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<peritus_types::Sha256Digest>, DebuggerError> {
    reader.read_option_tag().map_err(codec)?.then(|| digest(reader)).transpose()
}

fn digest(reader: &mut CanonicalReader<'_>) -> Result<peritus_types::Sha256Digest, DebuggerError> {
    Ok(peritus_types::Sha256Digest::new(reader.read_fixed().map_err(codec)?))
}

fn codec(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelProtocol,
        DebuggerOperation::DecodeProtocol,
        DebuggerRecovery::Quarantine,
        error.to_string(),
    )
}

fn protocol(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelProtocol,
        DebuggerOperation::DecodeProtocol,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
