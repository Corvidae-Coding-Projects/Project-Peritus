//! Strict canonical encoding for complete debugger checkpoints.

mod model;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::{
    AcceptanceSpecId, EventId, EvidenceId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerJobId, DebuggerOperation, DebuggerRecovery, ReportId,
    SelectionManifestId,
};

use super::super::{
    AnalysisCounts, DebuggerPhase, DebuggerState, JobFailure, JobFailureCode, PublicationRecord,
    ReportRecord, SelectionRecord,
};
use model::{decode_model, decode_model_attempts, encode_model, encode_model_attempts};

const STATE_DOMAIN: &[u8] = b"peritus.debugger.state.v1\0";

pub(super) fn encode(state: &DebuggerState) -> Result<Vec<u8>, DebuggerError> {
    let identity = identity_bytes(state)?;
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(&identity).map_err(codec)?;
    writer.write_fixed(state.state_digest().as_bytes()).map_err(codec)?;
    Ok(writer.into_bytes())
}

pub(super) fn identity_bytes(state: &DebuggerState) -> Result<Vec<u8>, DebuggerError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(STATE_DOMAIN).map_err(codec)?;
    writer.write_fixed(state.job_id().as_bytes()).map_err(codec)?;
    crate::aggregate::encode_revision(&mut writer, state.revision())?;
    writer.write_fixed(state.query_digest().as_bytes()).map_err(codec)?;
    writer.write_fixed(state.limits_digest().as_bytes()).map_err(codec)?;
    optional_digest(&mut writer, state.model_plan_digest())?;
    writer.write_u64(state.sequence()).map_err(codec)?;
    writer.write_fixed(state.last_event_id().as_bytes()).map_err(codec)?;
    writer.write_u8(state.phase().tag()).map_err(codec)?;
    encode_selection(&mut writer, state.selection())?;
    optional_digest(&mut writer, state.deterministic_digest())?;
    encode_analysis_counts(&mut writer, state.analysis_counts())?;
    encode_model(&mut writer, state.model())?;
    encode_model_attempts(&mut writer, state.model_attempts())?;
    encode_report(&mut writer, state.report())?;
    encode_publication(&mut writer, state.publication())?;
    encode_job_failure(&mut writer, state.failure())?;
    optional_digest(&mut writer, state.cancellation_reason_digest())?;
    Ok(writer.into_bytes())
}

pub(super) fn decode(bytes: &[u8]) -> Result<DebuggerState, DebuggerError> {
    let mut outer = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let identity = outer.read_bytes().map_err(codec)?;
    let advertised = digest(&mut outer)?;
    outer.finish().map_err(codec)?;
    if peritus_codec::sha256(identity) != advertised {
        return Err(corrupt("state digest differs from complete canonical fields"));
    }
    let mut reader = CanonicalReader::new(identity, CodecLimits::PRODUCTION);
    let domain = reader.read_fixed::<26>().map_err(codec)?;
    if domain.as_slice() != STATE_DOMAIN {
        return Err(corrupt("state domain or schema version is unsupported"));
    }
    let job_id = DebuggerJobId::new(reader.read_fixed().map_err(codec)?)?;
    let revision = decode_revision(&mut reader)?;
    let query_digest = digest(&mut reader)?;
    let limits_digest = digest(&mut reader)?;
    let model_plan_digest = optional_digest_read(&mut reader)?;
    let sequence = reader.read_u64().map_err(codec)?;
    if sequence == 0 {
        return Err(corrupt("materialized debugger state has zero sequence"));
    }
    let last_event_id = event_id(&mut reader)?;
    let phase = DebuggerPhase::from_tag(reader.read_u8().map_err(codec)?)?;
    let selection = decode_selection(&mut reader)?;
    let deterministic_digest = optional_digest_read(&mut reader)?;
    let analysis_counts = decode_analysis_counts(&mut reader)?;
    let model = decode_model(&mut reader)?;
    let model_attempts = decode_model_attempts(&mut reader)?;
    let report = decode_report(&mut reader)?;
    let publication = decode_publication(&mut reader)?;
    let failure = decode_job_failure(&mut reader)?;
    let cancellation_reason_digest = optional_digest_read(&mut reader)?;
    reader.finish().map_err(codec)?;
    let state = DebuggerState {
        job_id,
        revision,
        query_digest,
        limits_digest,
        model_plan_digest,
        sequence,
        last_event_id,
        state_digest: advertised,
        phase,
        selection,
        deterministic_digest,
        analysis_counts,
        model,
        model_attempts,
        report,
        publication,
        failure,
        cancellation_reason_digest,
    };
    validate_shape(&state)?;
    Ok(state)
}

fn validate_shape(state: &DebuggerState) -> Result<(), DebuggerError> {
    let complete_analysis =
        state.deterministic_digest().is_some() && state.analysis_counts().is_some();
    let valid = match state.phase() {
        DebuggerPhase::Created => state.selection().is_none() && !complete_analysis,
        DebuggerPhase::Selected => state.selection().is_some() && !complete_analysis,
        DebuggerPhase::DeterministicComplete => state.selection().is_some() && complete_analysis,
        DebuggerPhase::ModelPending
        | DebuggerPhase::ModelRunning
        | DebuggerPhase::ModelValidated => {
            state.selection().is_some() && complete_analysis && state.model().is_some()
        }
        DebuggerPhase::ReportReady => state.report().is_some() && state.publication().is_none(),
        DebuggerPhase::Published => state.report().is_some() && state.publication().is_some(),
        DebuggerPhase::Failed => state.failure().is_some(),
        DebuggerPhase::Cancelled => state.cancellation_reason_digest().is_some(),
    };
    if valid { Ok(()) } else { Err(corrupt("state fields contradict the durable job phase")) }
}

fn encode_selection(
    writer: &mut CanonicalWriter,
    selection: Option<SelectionRecord>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(selection.is_some()).map_err(codec)?;
    if let Some(value) = selection {
        writer.write_fixed(value.id().as_bytes()).map_err(codec)?;
        writer.write_fixed(value.digest().as_bytes()).map_err(codec)?;
        writer.write_u64(value.subjects()).map_err(codec)?;
        writer.write_u64(value.events()).map_err(codec)?;
    }
    Ok(())
}

fn decode_selection(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<SelectionRecord>, DebuggerError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    SelectionRecord::new(
        SelectionManifestId::new(reader.read_fixed().map_err(codec)?)?,
        digest(reader)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )
    .map(Some)
}

fn encode_analysis_counts(
    writer: &mut CanonicalWriter,
    counts: Option<AnalysisCounts>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(counts.is_some()).map_err(codec)?;
    if let Some(value) = counts {
        crate::aggregate::encode_counts(writer, value)?;
    }
    Ok(())
}

fn decode_analysis_counts(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<AnalysisCounts>, DebuggerError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    Ok(Some(AnalysisCounts::new(
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )))
}

fn encode_report(
    writer: &mut CanonicalWriter,
    report: Option<ReportRecord>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(report.is_some()).map_err(codec)?;
    if let Some(value) = report {
        crate::aggregate::encode_report(writer, value)?;
    }
    Ok(())
}

fn decode_report(reader: &mut CanonicalReader<'_>) -> Result<Option<ReportRecord>, DebuggerError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    ReportRecord::new(
        ReportId::new(reader.read_fixed().map_err(codec)?)?,
        digest(reader)?,
        reader.read_u64().map_err(codec)?,
    )
    .map(Some)
}

fn encode_publication(
    writer: &mut CanonicalWriter,
    publication: Option<PublicationRecord>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(publication.is_some()).map_err(codec)?;
    if let Some(value) = publication {
        crate::aggregate::encode_publication(writer, value)?;
    }
    Ok(())
}

fn decode_publication(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<PublicationRecord>, DebuggerError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    PublicationRecord::new(
        ReportId::new(reader.read_fixed().map_err(codec)?)?,
        digest(reader)?,
        reader.read_u64().map_err(codec)?,
        EvidenceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid evidence identity"))?,
        reader.read_u64().map_err(codec)?,
    )
    .map(Some)
}

fn encode_job_failure(
    writer: &mut CanonicalWriter,
    failure: Option<JobFailure>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(failure.is_some()).map_err(codec)?;
    if let Some(value) = failure {
        writer.write_u8(value.code().tag()).map_err(codec)?;
        writer.write_fixed(value.diagnostic_digest().as_bytes()).map_err(codec)?;
    }
    Ok(())
}

fn decode_job_failure(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<JobFailure>, DebuggerError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    Ok(Some(JobFailure::new(
        JobFailureCode::from_tag(reader.read_u8().map_err(codec)?)?,
        digest(reader)?,
    )))
}

fn decode_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, DebuggerError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid acceptance identity"))?,
        HarnessId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid harness identity"))?,
        WorkspaceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid workspace identity"))?,
        Generation::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| corrupt("invalid workspace generation"))?,
        RevisionNumber::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| corrupt("invalid workspace revision"))?,
        PolicyId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid policy identity"))?,
        ProviderProfileId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid profile identity"))?,
    ))
}

fn optional_digest(
    writer: &mut CanonicalWriter,
    value: Option<Sha256Digest>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(value.is_some()).map_err(codec)?;
    if let Some(digest) = value {
        writer.write_fixed(digest.as_bytes()).map_err(codec)?;
    }
    Ok(())
}

fn optional_digest_read(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<Sha256Digest>, DebuggerError> {
    reader.read_option_tag().map_err(codec)?.then(|| digest(reader)).transpose()
}

fn digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, DebuggerError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec)?))
}

fn event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, DebuggerError> {
    EventId::new(reader.read_fixed().map_err(codec)?).map_err(|_| corrupt("invalid event identity"))
}

fn codec(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::Replay,
        DebuggerRecovery::Quarantine,
        error.to_string(),
    )
}

fn corrupt(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::Replay,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
