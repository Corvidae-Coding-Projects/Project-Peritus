//! Canonical prompt binding, answer, and cancellation helpers.

use crate::{
    AppProtocolLimits, ApprovalAnswer, ApprovalChallenge, CorrelationId, PromptAnswer,
    PromptAnswerPayload, PromptBinding, PromptCancellation, PromptChoice, PromptConstraint,
    PromptCorrelation, PromptId, PromptKind, RequestId, SignedApprovalDecisionFrame,
    UserInputValue,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{ActorId, CommandId, Generation, RevisionNumber, SessionId};

use super::primitive::{
    invalid, read_digest, read_id, read_revision, read_string_option, unknown, write_digest,
    write_id, write_revision, write_string_option,
};

pub(super) fn write_prompt_correlation(
    writer: &mut CanonicalWriter,
    value: PromptCorrelation,
) -> Result<(), CodecError> {
    write_id(writer, value.originating_request_id().as_bytes())?;
    write_id(writer, value.prompt_id().as_bytes())?;
    write_id(writer, value.session_id().as_bytes())?;
    write_id(writer, value.actor_id().as_bytes())?;
    write_revision(writer, value.revision())?;
    write_digest(writer, value.freshness_digest())?;
    writer.write_u64(value.cancellation_generation().get())
}

pub(super) fn read_prompt_correlation(
    reader: &mut CanonicalReader<'_>,
) -> Result<PromptCorrelation, CodecError> {
    let request_id = read_id(reader, RequestId::new)?;
    let prompt_id = read_id(reader, PromptId::new)?;
    let session_id = read_id(reader, SessionId::new)?;
    let actor_id = read_id(reader, ActorId::new)?;
    let revision = read_revision(reader)?;
    let digest = read_digest(reader)?;
    let generation_offset = reader.offset();
    let generation = invalid(generation_offset, Generation::new(reader.read_u64()?))?;
    Ok(PromptCorrelation::new(
        request_id, prompt_id, session_id, actor_id, revision, digest, generation,
    ))
}

pub(super) fn write_prompt_binding(
    writer: &mut CanonicalWriter,
    value: &PromptBinding,
) -> Result<(), CodecError> {
    writer.write_u8(prompt_kind_tag(value.kind()))?;
    write_prompt_correlation(writer, value.correlation())?;
    writer.write_option_tag(value.approval_challenge().is_some())?;
    if let Some(challenge) = value.approval_challenge() {
        write_approval_challenge(writer, challenge)?;
    }
    writer.write_collection_len(value.choices().len())?;
    for choice in value.choices() {
        writer.write_str(choice.id())?;
        writer.write_str(choice.label())?;
    }
    writer.write_collection_len(value.constraints().len())?;
    for constraint in value.constraints() {
        match constraint {
            PromptConstraint::NonEmpty => writer.write_u8(1)?,
            PromptConstraint::MaximumTextBytes(maximum) => {
                writer.write_u8(2)?;
                writer.write_u32(*maximum)?;
            }
            PromptConstraint::BoundChoiceOnly => writer.write_u8(3)?,
            PromptConstraint::SecretReference => writer.write_u8(4)?,
        }
    }
    Ok(())
}

pub(super) fn read_prompt_binding(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<PromptBinding, CodecError> {
    let offset = reader.offset();
    let kind = read_prompt_kind(reader)?;
    let correlation = read_prompt_correlation(reader)?;
    let challenge = if reader.read_option_tag()? {
        Some(read_approval_challenge(reader, limits)?)
    } else {
        None
    };
    let choice_count = reader.read_collection_len()?;
    if choice_count > limits.max_prompt_choices() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut choices = Vec::with_capacity(choice_count);
    for _ in 0..choice_count {
        let choice_offset = reader.offset();
        choices.push(invalid(
            choice_offset,
            PromptChoice::new(
                reader.read_str()?.to_owned(),
                reader.read_str()?.to_owned(),
                limits.codec().max_string_bytes,
                limits.codec().max_string_bytes,
            ),
        )?);
    }
    let constraint_count = reader.read_collection_len()?;
    let mut constraints = Vec::with_capacity(constraint_count);
    for _ in 0..constraint_count {
        let tag_offset = reader.offset();
        constraints.push(match reader.read_u8()? {
            1 => PromptConstraint::NonEmpty,
            2 => PromptConstraint::MaximumTextBytes(reader.read_u32()?),
            3 => PromptConstraint::BoundChoiceOnly,
            4 => PromptConstraint::SecretReference,
            _ => return unknown(tag_offset),
        });
    }
    let binding = match (kind, challenge) {
        (PromptKind::Approval, Some(challenge)) if choices.is_empty() => PromptBinding::approval(
            correlation,
            challenge,
            constraints,
            limits.codec().max_collection_items,
        ),
        (PromptKind::UserInput, None) => PromptBinding::new(
            kind,
            correlation,
            choices,
            constraints,
            limits.max_prompt_choices(),
            limits.codec().max_collection_items,
        ),
        _ => return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset)),
    };
    invalid(offset, binding)
}

pub(super) fn write_prompt_answer(
    writer: &mut CanonicalWriter,
    value: &PromptAnswer,
) -> Result<(), CodecError> {
    write_prompt_correlation(writer, value.correlation())?;
    match value.payload() {
        PromptAnswerPayload::Approval { answer, rationale } => {
            writer.write_u8(1)?;
            match answer {
                ApprovalAnswer::SignedDecision(frame) => {
                    writer.write_u8(1)?;
                    writer.write_bytes(frame.bytes())?;
                }
                ApprovalAnswer::Cancel => writer.write_u8(2)?,
            }
            write_string_option(writer, rationale.as_deref())
        }
        PromptAnswerPayload::UserInput(input) => {
            writer.write_u8(2)?;
            write_user_input(writer, input)
        }
    }
}

pub(super) fn read_prompt_answer(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<PromptAnswer, CodecError> {
    let offset = reader.offset();
    let correlation = read_prompt_correlation(reader)?;
    let tag_offset = reader.offset();
    let payload = match reader.read_u8()? {
        1 => {
            let answer_offset = reader.offset();
            let answer = match reader.read_u8()? {
                1 => ApprovalAnswer::SignedDecision(invalid(
                    answer_offset,
                    SignedApprovalDecisionFrame::new(
                        reader.read_bytes_owned()?,
                        limits.codec().max_opaque_bytes,
                    ),
                )?),
                2 => ApprovalAnswer::Cancel,
                _ => return unknown(answer_offset),
            };
            let rationale = read_string_option(reader)?;
            match answer {
                ApprovalAnswer::SignedDecision(frame) => invalid(
                    offset,
                    PromptAnswerPayload::signed_approval(
                        frame,
                        rationale,
                        limits.codec().max_string_bytes,
                    ),
                )?,
                ApprovalAnswer::Cancel => invalid(
                    offset,
                    PromptAnswerPayload::cancel_approval(
                        rationale,
                        limits.codec().max_string_bytes,
                    ),
                )?,
            }
        }
        2 => PromptAnswerPayload::UserInput(read_user_input(reader, limits)?),
        _ => return unknown(tag_offset),
    };
    invalid(offset, PromptAnswer::new(correlation, payload, limits.codec().max_string_bytes))
}

fn write_approval_challenge(
    writer: &mut CanonicalWriter,
    value: &ApprovalChallenge,
) -> Result<(), CodecError> {
    write_id(writer, value.decision_command_id().as_bytes())?;
    writer.write_u64(value.registry_revision().get())?;
    writer.write_bytes(value.request_frame())
}

fn read_approval_challenge(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<ApprovalChallenge, CodecError> {
    let offset = reader.offset();
    let command_id = read_id(reader, CommandId::new)?;
    let revision = invalid(offset, RevisionNumber::new(reader.read_u64()?))?;
    let frame = reader.read_bytes_owned()?;
    invalid(
        offset,
        ApprovalChallenge::new(command_id, revision, frame, limits.codec().max_opaque_bytes),
    )
}

pub(super) fn write_prompt_cancellation(
    writer: &mut CanonicalWriter,
    value: PromptCancellation,
) -> Result<(), CodecError> {
    write_prompt_correlation(writer, value.correlation())?;
    write_id(writer, value.correlation_id().as_bytes())
}

pub(super) fn read_prompt_cancellation(
    reader: &mut CanonicalReader<'_>,
) -> Result<PromptCancellation, CodecError> {
    Ok(PromptCancellation::new(
        read_prompt_correlation(reader)?,
        read_id(reader, CorrelationId::new)?,
    ))
}

fn write_user_input(
    writer: &mut CanonicalWriter,
    value: &UserInputValue,
) -> Result<(), CodecError> {
    match value {
        UserInputValue::Text(value) => {
            writer.write_u8(1)?;
            writer.write_str(value)
        }
        UserInputValue::Selection(value) => {
            writer.write_u8(2)?;
            writer.write_str(value)
        }
        UserInputValue::Confirmation(value) => {
            writer.write_u8(3)?;
            writer.write_bool(*value)
        }
        UserInputValue::SecretReference(value) => {
            writer.write_u8(4)?;
            writer.write_str(value)
        }
    }
}

fn read_user_input(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<UserInputValue, CodecError> {
    let offset = reader.offset();
    let maximum = limits.codec().max_string_bytes;
    match reader.read_u8()? {
        1 => invalid(offset, UserInputValue::text(reader.read_str()?.to_owned(), maximum)),
        2 => invalid(offset, UserInputValue::selection(reader.read_str()?.to_owned(), maximum)),
        3 => Ok(UserInputValue::confirmation(reader.read_bool()?)),
        4 => invalid(
            offset,
            UserInputValue::secret_reference(reader.read_str()?.to_owned(), maximum),
        ),
        _ => unknown(offset),
    }
}

const fn prompt_kind_tag(value: PromptKind) -> u8 {
    match value {
        PromptKind::Approval => 1,
        PromptKind::UserInput => 2,
    }
}

fn read_prompt_kind(reader: &mut CanonicalReader<'_>) -> Result<PromptKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(PromptKind::Approval),
        2 => Ok(PromptKind::UserInput),
        _ => unknown(offset),
    }
}
