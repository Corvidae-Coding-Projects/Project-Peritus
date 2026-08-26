//! Canonical schema-v1 application request family.

use crate::{
    APP_SCHEMA_V1, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, ArtifactOpenRequest,
    CorrelationId, EventCursor, REQUEST_FAMILY, RequestId, SubscriptionId, SubscriptionRequest,
    TransferId,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::ArtifactId;

use super::{
    artifact::{read_artifact_cancellation, write_artifact_cancellation},
    command::{read_command_binding, write_command_binding},
    daemon::{read_shutdown_request, write_shutdown_request},
    primitive::{invalid, read_context, read_id, unknown, write_context, write_id},
    prompt::{
        read_prompt_answer, read_prompt_cancellation, write_prompt_answer,
        write_prompt_cancellation,
    },
    subscription::{read_filter, write_filter},
    terminal::{
        read_terminal_binding, read_terminal_cancellation, read_terminal_detach,
        read_terminal_input, read_terminal_resize, write_terminal_binding,
        write_terminal_cancellation, write_terminal_detach, write_terminal_input,
        write_terminal_resize,
    },
};

impl CanonicalEncode for AppRequestEnvelope {
    const FAMILY: u16 = REQUEST_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        validate_request_binding(self, writer.len())?;
        write_context(writer, self.context())?;
        write_id(writer, self.request_id().as_bytes())?;
        write_id(writer, self.correlation_id().as_bytes())?;
        match self.payload() {
            AppRequestPayload::SubmitCommand(value) => {
                writer.write_u16(1)?;
                write_command_binding(writer, value)
            }
            AppRequestPayload::Subscribe(value) => {
                writer.write_u16(2)?;
                write_subscription_request(writer, value)
            }
            AppRequestPayload::OpenArtifact(value) => {
                writer.write_u16(3)?;
                write_artifact_open(writer, *value)
            }
            AppRequestPayload::CancelArtifact(value) => {
                writer.write_u16(4)?;
                write_artifact_cancellation(writer, *value)
            }
            AppRequestPayload::AnswerPrompt(value) => {
                writer.write_u16(5)?;
                write_prompt_answer(writer, value)
            }
            AppRequestPayload::CancelPrompt(value) => {
                writer.write_u16(6)?;
                write_prompt_cancellation(writer, *value)
            }
            AppRequestPayload::AttachTerminal(value) => {
                writer.write_u16(7)?;
                write_terminal_binding(writer, *value)
            }
            AppRequestPayload::TerminalInput(value) => {
                writer.write_u16(8)?;
                write_terminal_input(writer, value)
            }
            AppRequestPayload::TerminalResize(value) => {
                writer.write_u16(9)?;
                write_terminal_resize(writer, *value)
            }
            AppRequestPayload::DetachTerminal(value) => {
                writer.write_u16(10)?;
                write_terminal_detach(writer, *value)
            }
            AppRequestPayload::CancelTerminal(value) => {
                writer.write_u16(11)?;
                write_terminal_cancellation(writer, *value)
            }
            AppRequestPayload::DaemonStatus => writer.write_u16(12),
            AppRequestPayload::Shutdown(value) => {
                writer.write_u16(13)?;
                write_shutdown_request(writer, *value)
            }
        }
    }
}

impl CanonicalDecode for AppRequestEnvelope {
    const FAMILY: u16 = REQUEST_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_request(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_request(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<AppRequestEnvelope, CodecError> {
    let offset = reader.offset();
    let context = read_context(reader)?;
    let request_id = read_id(reader, RequestId::new)?;
    let correlation_id = read_id(reader, CorrelationId::new)?;
    let tag_offset = reader.offset();
    let payload = match reader.read_u16()? {
        1 => AppRequestPayload::SubmitCommand(read_command_binding(reader, limits)?),
        2 => AppRequestPayload::Subscribe(read_subscription_request(reader, limits)?),
        3 => AppRequestPayload::OpenArtifact(read_artifact_open(reader)?),
        4 => AppRequestPayload::CancelArtifact(read_artifact_cancellation(reader)?),
        5 => AppRequestPayload::AnswerPrompt(read_prompt_answer(reader, limits)?),
        6 => AppRequestPayload::CancelPrompt(read_prompt_cancellation(reader)?),
        7 => AppRequestPayload::AttachTerminal(read_terminal_binding(reader)?),
        8 => AppRequestPayload::TerminalInput(read_terminal_input(reader, limits)?),
        9 => AppRequestPayload::TerminalResize(read_terminal_resize(reader)?),
        10 => AppRequestPayload::DetachTerminal(read_terminal_detach(reader)?),
        11 => AppRequestPayload::CancelTerminal(read_terminal_cancellation(reader)?),
        12 => AppRequestPayload::DaemonStatus,
        13 => AppRequestPayload::Shutdown(read_shutdown_request(reader)?),
        _ => return unknown(tag_offset),
    };
    let request =
        invalid(offset, AppRequestEnvelope::new(context, request_id, correlation_id, payload))?;
    validate_request_binding(&request, offset)?;
    Ok(request)
}

fn write_subscription_request(
    writer: &mut CanonicalWriter,
    value: &SubscriptionRequest,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    write_filter(writer, value.filter())?;
    writer.write_u64(value.after().get())?;
    writer.write_u32(value.maximum_in_flight())?;
    writer.write_bool(value.snapshot_acceptable())
}

fn read_subscription_request(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<SubscriptionRequest, CodecError> {
    let offset = reader.offset();
    let subscription_id = read_id(reader, SubscriptionId::new)?;
    let filter = read_filter(reader, limits)?;
    let after = EventCursor::new(reader.read_u64()?);
    let maximum = reader.read_u32()?;
    let maximum_usize = usize::try_from(maximum)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))?;
    if maximum_usize > limits.max_in_flight_events() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let snapshot_acceptable = reader.read_bool()?;
    invalid(
        offset,
        SubscriptionRequest::new(subscription_id, filter, after, maximum, snapshot_acceptable),
    )
}

fn write_artifact_open(
    writer: &mut CanonicalWriter,
    value: ArtifactOpenRequest,
) -> Result<(), CodecError> {
    write_id(writer, value.transfer_id().as_bytes())?;
    write_id(writer, value.artifact_id().as_bytes())
}

fn read_artifact_open(reader: &mut CanonicalReader<'_>) -> Result<ArtifactOpenRequest, CodecError> {
    Ok(ArtifactOpenRequest::new(
        read_id(reader, TransferId::new)?,
        read_id(reader, ArtifactId::new)?,
    ))
}

fn validate_request_binding(value: &AppRequestEnvelope, offset: usize) -> Result<(), CodecError> {
    let matches = match value.payload() {
        AppRequestPayload::AttachTerminal(inner) => {
            inner.originating_request_id() == value.request_id()
        }
        AppRequestPayload::CancelArtifact(inner) => {
            inner.correlation_id() == value.correlation_id()
        }
        AppRequestPayload::AnswerPrompt(inner) => {
            inner.correlation().session_id() == value.context().session_id()
        }
        AppRequestPayload::CancelPrompt(inner) => {
            if inner.correlation_id() == value.correlation_id() {
                inner.correlation().session_id() == value.context().session_id()
            } else {
                false
            }
        }
        AppRequestPayload::DetachTerminal(inner) => {
            inner.correlation_id() == value.correlation_id()
        }
        AppRequestPayload::CancelTerminal(inner) => {
            inner.correlation_id() == value.correlation_id()
        }
        _ => true,
    };
    if matches { Ok(()) } else { Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset)) }
}
