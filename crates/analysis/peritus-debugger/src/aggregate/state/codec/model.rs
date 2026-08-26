//! Canonical optional-model progress and attempt-history checkpoint fields.

use peritus_codec::{CanonicalReader, CanonicalWriter};

use crate::{DebuggerError, ModelAnalysisId};

use super::super::super::{
    ModelAttemptFailure, ModelAttemptFailureCode, ModelAttemptObservation, ModelAttemptResult,
    ModelBudget, ModelProgress, ModelRetryPolicy, ModelWorkState,
};
use super::{codec, corrupt, digest};

pub(super) fn encode_model(
    writer: &mut CanonicalWriter,
    model: Option<ModelProgress>,
) -> Result<(), DebuggerError> {
    writer.write_option_tag(model.is_some()).map_err(codec)?;
    let Some(value) = model else {
        return Ok(());
    };
    writer.write_fixed(value.id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.plan_digest().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.request_digest().as_bytes()).map_err(codec)?;
    crate::aggregate::encode_model_budget(writer, value.budget())?;
    crate::aggregate::encode_retry_policy(writer, value.retry_policy())?;
    match value.state() {
        ModelWorkState::Pending { attempt, not_before_tick } => {
            writer.write_u8(1).map_err(codec)?;
            writer.write_u16(attempt).map_err(codec)?;
            writer.write_u64(not_before_tick).map_err(codec)
        }
        ModelWorkState::Running { attempt, started_at_tick } => {
            writer.write_u8(2).map_err(codec)?;
            writer.write_u16(attempt).map_err(codec)?;
            writer.write_u64(started_at_tick).map_err(codec)
        }
        ModelWorkState::AwaitingRetry { attempt, failure } => {
            writer.write_u8(3).map_err(codec)?;
            writer.write_u16(attempt).map_err(codec)?;
            crate::aggregate::encode_model_failure(writer, failure)
        }
        ModelWorkState::Validated { attempt, proposal_digest } => {
            writer.write_u8(4).map_err(codec)?;
            writer.write_u16(attempt).map_err(codec)?;
            writer.write_fixed(proposal_digest.as_bytes()).map_err(codec)
        }
        ModelWorkState::Rejected { attempt, failure } => {
            writer.write_u8(5).map_err(codec)?;
            writer.write_u16(attempt).map_err(codec)?;
            crate::aggregate::encode_model_failure(writer, failure)
        }
    }
}

pub(super) fn decode_model(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<ModelProgress>, DebuggerError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    let id = ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?;
    let plan_digest = digest(reader)?;
    let request_digest = digest(reader)?;
    let budget = decode_model_budget(reader)?;
    let retry = ModelRetryPolicy::new(
        reader.read_u16().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )?;
    let state = match reader.read_u8().map_err(codec)? {
        1 => ModelWorkState::Pending {
            attempt: nonzero_attempt(reader.read_u16().map_err(codec)?)?,
            not_before_tick: reader.read_u64().map_err(codec)?,
        },
        2 => ModelWorkState::Running {
            attempt: nonzero_attempt(reader.read_u16().map_err(codec)?)?,
            started_at_tick: nonzero_tick(reader.read_u64().map_err(codec)?)?,
        },
        3 => {
            let attempt = nonzero_attempt(reader.read_u16().map_err(codec)?)?;
            ModelWorkState::AwaitingRetry { attempt, failure: decode_model_failure(reader)? }
        }
        4 => ModelWorkState::Validated {
            attempt: nonzero_attempt(reader.read_u16().map_err(codec)?)?,
            proposal_digest: digest(reader)?,
        },
        5 => {
            let attempt = nonzero_attempt(reader.read_u16().map_err(codec)?)?;
            ModelWorkState::Rejected { attempt, failure: decode_model_failure(reader)? }
        }
        _ => return Err(corrupt("unknown model-work state tag")),
    };
    let progress =
        ModelProgress::new(id, plan_digest, request_digest, budget, retry).with_state(state);
    validate_model_state(progress)?;
    Ok(Some(progress))
}

pub(super) fn encode_model_attempts(
    writer: &mut CanonicalWriter,
    attempts: &[ModelAttemptObservation],
) -> Result<(), DebuggerError> {
    writer.write_collection_len(attempts.len()).map_err(codec)?;
    for attempt in attempts {
        writer.write_fixed(attempt.model_id().as_bytes()).map_err(codec)?;
        writer.write_u16(attempt.attempt()).map_err(codec)?;
        match attempt.result() {
            ModelAttemptResult::Proposal {
                proposal_digest,
                output_digest,
                output_bytes,
                event_count,
                input_tokens,
                output_tokens,
                total_tokens,
            } => {
                writer.write_u8(1).map_err(codec)?;
                writer.write_fixed(proposal_digest.as_bytes()).map_err(codec)?;
                writer.write_fixed(output_digest.as_bytes()).map_err(codec)?;
                writer.write_u64(output_bytes).map_err(codec)?;
                writer.write_u64(event_count).map_err(codec)?;
                writer.write_u64(input_tokens).map_err(codec)?;
                writer.write_u64(output_tokens).map_err(codec)?;
                writer.write_u64(total_tokens).map_err(codec)?;
            }
            ModelAttemptResult::Failure(failure) => {
                writer.write_u8(2).map_err(codec)?;
                crate::aggregate::encode_model_failure(writer, failure)?;
            }
        }
    }
    Ok(())
}

pub(super) fn decode_model_attempts(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<ModelAttemptObservation>, DebuggerError> {
    let count = reader.read_collection_len().map_err(codec)?;
    if count > 32 {
        return Err(corrupt("model attempt history exceeds compiled bound"));
    }
    let mut attempts = Vec::with_capacity(count);
    for index in 0..count {
        let model_id = ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?;
        let attempt = nonzero_attempt(reader.read_u16().map_err(codec)?)?;
        if usize::from(attempt) != index + 1 {
            return Err(corrupt("model attempt history is not contiguous"));
        }
        let result = match reader.read_u8().map_err(codec)? {
            1 => ModelAttemptResult::Proposal {
                proposal_digest: digest(reader)?,
                output_digest: digest(reader)?,
                output_bytes: reader.read_u64().map_err(codec)?,
                event_count: reader.read_u64().map_err(codec)?,
                input_tokens: reader.read_u64().map_err(codec)?,
                output_tokens: reader.read_u64().map_err(codec)?,
                total_tokens: reader.read_u64().map_err(codec)?,
            },
            2 => ModelAttemptResult::Failure(decode_model_failure(reader)?),
            _ => return Err(corrupt("unknown model attempt result tag")),
        };
        attempts.push(ModelAttemptObservation::new(model_id, attempt, result)?);
    }
    Ok(attempts)
}

fn validate_model_state(model: ModelProgress) -> Result<(), DebuggerError> {
    let (attempt, nested) = match model.state() {
        ModelWorkState::Pending { attempt, .. }
        | ModelWorkState::Running { attempt, .. }
        | ModelWorkState::Validated { attempt, .. } => (attempt, None),
        ModelWorkState::AwaitingRetry { attempt, failure }
        | ModelWorkState::Rejected { attempt, failure } => (attempt, Some(failure)),
    };
    if attempt > model.retry_policy().max_attempts()
        || nested
            .is_some_and(|failure| failure.model_id() != model.id() || failure.attempt() != attempt)
    {
        return Err(corrupt("model work state contradicts its plan or attempt"));
    }
    Ok(())
}

fn decode_model_budget(reader: &mut CanonicalReader<'_>) -> Result<ModelBudget, DebuggerError> {
    ModelBudget::new(
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )
}

fn decode_model_failure(
    reader: &mut CanonicalReader<'_>,
) -> Result<ModelAttemptFailure, DebuggerError> {
    ModelAttemptFailure::new(
        ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?,
        reader.read_u16().map_err(codec)?,
        ModelAttemptFailureCode::from_tag(reader.read_u8().map_err(codec)?)?,
        reader.read_bool().map_err(codec)?,
        digest(reader)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )
}

fn nonzero_attempt(value: u16) -> Result<u16, DebuggerError> {
    if value == 0 { Err(corrupt("model attempt is zero")) } else { Ok(value) }
}

fn nonzero_tick(value: u64) -> Result<u64, DebuggerError> {
    if value == 0 { Err(corrupt("model attempt tick is zero")) } else { Ok(value) }
}
