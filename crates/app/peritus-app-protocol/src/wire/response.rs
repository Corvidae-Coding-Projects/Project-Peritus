//! Canonical schema-v1 application response family.

use crate::{
    APP_SCHEMA_V1, AppProtocolLimits, AppResponseEnvelope, AppResponsePayload, CorrelationId,
    EventCursor, OperationAcknowledgement, PromptId, RESPONSE_FAMILY, RequestId, SubscriptionId,
    SubscriptionStarted,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use super::{
    artifact::{read_artifact_metadata, write_artifact_metadata},
    command::{read_command_result, write_command_result},
    daemon::{
        read_daemon_status, read_shutdown_accepted, write_daemon_status, write_shutdown_accepted,
    },
    error::{read_app_error, write_app_error},
    primitive::{read_context, read_id, unknown, write_context, write_id},
    terminal::{read_terminal_binding, write_terminal_binding},
};

impl CanonicalEncode for AppResponseEnvelope {
    const FAMILY: u16 = RESPONSE_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        validate_response_binding(self, writer.len())?;
        write_context(writer, self.context())?;
        write_id(writer, self.request_id().as_bytes())?;
        write_id(writer, self.correlation_id().as_bytes())?;
        match self.payload() {
            AppResponsePayload::CommandResult(value) => {
                writer.write_u16(1)?;
                write_command_result(writer, value)
            }
            AppResponsePayload::SubscriptionStarted(value) => {
                writer.write_u16(2)?;
                write_subscription_started(writer, *value)
            }
            AppResponsePayload::ArtifactOpened(value) => {
                writer.write_u16(3)?;
                write_artifact_metadata(writer, value)
            }
            AppResponsePayload::PromptAccepted(value) => {
                writer.write_u16(4)?;
                write_id(writer, value.as_bytes())
            }
            AppResponsePayload::TerminalAttached(value) => {
                writer.write_u16(5)?;
                write_terminal_binding(writer, *value)
            }
            AppResponsePayload::Acknowledged(value) => {
                writer.write_u16(6)?;
                write_id(writer, value.request_id().as_bytes())
            }
            AppResponsePayload::DaemonStatus(value) => {
                writer.write_u16(7)?;
                write_daemon_status(writer, value)
            }
            AppResponsePayload::ShutdownAccepted(value) => {
                writer.write_u16(8)?;
                write_shutdown_accepted(writer, *value)
            }
            AppResponsePayload::Error(value) => {
                writer.write_u16(9)?;
                write_app_error(writer, value)
            }
        }
    }
}

impl CanonicalDecode for AppResponseEnvelope {
    const FAMILY: u16 = RESPONSE_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_response(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_response(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<AppResponseEnvelope, CodecError> {
    let offset = reader.offset();
    let context = read_context(reader)?;
    let request_id = read_id(reader, RequestId::new)?;
    let correlation_id = read_id(reader, CorrelationId::new)?;
    let tag_offset = reader.offset();
    let payload = match reader.read_u16()? {
        1 => AppResponsePayload::CommandResult(read_command_result(reader, limits)?),
        2 => AppResponsePayload::SubscriptionStarted(read_subscription_started(reader, limits)?),
        3 => AppResponsePayload::ArtifactOpened(read_artifact_metadata(reader, limits)?),
        4 => AppResponsePayload::PromptAccepted(read_id(reader, PromptId::new)?),
        5 => AppResponsePayload::TerminalAttached(read_terminal_binding(reader)?),
        6 => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(read_id(
            reader,
            RequestId::new,
        )?)),
        7 => AppResponsePayload::DaemonStatus(read_daemon_status(reader, limits)?),
        8 => AppResponsePayload::ShutdownAccepted(read_shutdown_accepted(reader)?),
        9 => AppResponsePayload::Error(read_app_error(reader, limits)?),
        _ => return unknown(tag_offset),
    };
    let response = AppResponseEnvelope::new(context, request_id, correlation_id, payload);
    validate_response_binding(&response, offset)?;
    Ok(response)
}

fn write_subscription_started(
    writer: &mut CanonicalWriter,
    value: SubscriptionStarted,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    writer.write_u64(value.after().get())?;
    writer.write_u32(value.maximum_in_flight())
}

fn read_subscription_started(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<SubscriptionStarted, CodecError> {
    let offset = reader.offset();
    let subscription_id = read_id(reader, SubscriptionId::new)?;
    let after = EventCursor::new(reader.read_u64()?);
    let maximum = reader.read_u32()?;
    let maximum_usize = usize::try_from(maximum)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))?;
    if maximum_usize > limits.max_in_flight_events() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    if maximum == 0 {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(SubscriptionStarted::new(subscription_id, after, maximum))
}

fn validate_response_binding(
    response: &AppResponseEnvelope,
    offset: usize,
) -> Result<(), CodecError> {
    let matches = match response.payload() {
        AppResponsePayload::CommandResult(value) => {
            value.original_request_id() == response.request_id()
        }
        AppResponsePayload::TerminalAttached(value) => {
            value.originating_request_id() == response.request_id()
        }
        AppResponsePayload::Acknowledged(value) => value.request_id() == response.request_id(),
        AppResponsePayload::ShutdownAccepted(value) => {
            value.request().request_id() == response.request_id()
                && value.request().correlation_id() == response.correlation_id()
        }
        _ => true,
    };
    if matches { Ok(()) } else { Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset)) }
}
