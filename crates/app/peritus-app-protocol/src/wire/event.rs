//! Canonical schema-v1 application event family.

use crate::{
    APP_SCHEMA_V1, AppDiagnostic, AppEventEnvelope, AppEventPayload, AppProtocolLimits,
    EVENT_FAMILY, SubscriptionId,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use super::{
    artifact::{
        read_artifact_chunk, read_artifact_completion, read_artifact_metadata,
        write_artifact_chunk, write_artifact_completion, write_artifact_metadata,
    },
    daemon::{
        read_daemon_status, read_heartbeat, read_shutdown_complete, read_shutdown_progress,
        write_daemon_status, write_heartbeat, write_shutdown_complete, write_shutdown_progress,
    },
    primitive::{invalid, read_context, read_id, unknown, write_context, write_id},
    prompt::{read_prompt_binding, write_prompt_binding},
    subscription::{
        read_backpressure, read_delivery, read_gap, write_backpressure, write_delivery, write_gap,
    },
    terminal::{
        read_terminal_exit, read_terminal_output, write_terminal_exit, write_terminal_output,
    },
};

impl CanonicalEncode for AppEventEnvelope {
    const FAMILY: u16 = EVENT_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        if let AppEventPayload::PromptRequested(prompt) = self.payload()
            && prompt.correlation().session_id() != self.context().session_id()
        {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len()));
        }
        write_context(writer, self.context())?;
        match self.payload() {
            AppEventPayload::DomainEvent(value) => {
                writer.write_u16(1)?;
                write_delivery(writer, value)
            }
            AppEventPayload::SubscriptionGap { subscription_id, gap } => {
                writer.write_u16(2)?;
                write_id(writer, subscription_id.as_bytes())?;
                write_gap(writer, *gap)
            }
            AppEventPayload::Backpressure(value) => {
                writer.write_u16(3)?;
                write_backpressure(writer, *value)
            }
            AppEventPayload::ArtifactMetadata(value) => {
                writer.write_u16(4)?;
                write_artifact_metadata(writer, value)
            }
            AppEventPayload::ArtifactChunk(value) => {
                writer.write_u16(5)?;
                write_artifact_chunk(writer, value)
            }
            AppEventPayload::ArtifactComplete(value) => {
                writer.write_u16(6)?;
                write_artifact_completion(writer, *value)
            }
            AppEventPayload::PromptRequested(value) => {
                writer.write_u16(7)?;
                write_prompt_binding(writer, value)
            }
            AppEventPayload::TerminalOutput(value) => {
                writer.write_u16(8)?;
                write_terminal_output(writer, value)
            }
            AppEventPayload::TerminalExited(value) => {
                writer.write_u16(9)?;
                write_terminal_exit(writer, *value)
            }
            AppEventPayload::ReadinessChanged(value) => {
                writer.write_u16(10)?;
                write_daemon_status(writer, value)
            }
            AppEventPayload::Diagnostic(value) => {
                writer.write_u16(11)?;
                writer.write_str(value.as_str())
            }
            AppEventPayload::Heartbeat(value) => {
                writer.write_u16(12)?;
                write_heartbeat(writer, value)
            }
            AppEventPayload::ShutdownProgress(value) => {
                writer.write_u16(13)?;
                write_shutdown_progress(writer, value)
            }
            AppEventPayload::ShutdownComplete(value) => {
                writer.write_u16(14)?;
                write_shutdown_complete(writer, value)
            }
        }
    }
}

impl CanonicalDecode for AppEventEnvelope {
    const FAMILY: u16 = EVENT_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_event(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_event(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<AppEventEnvelope, CodecError> {
    let context = read_context(reader)?;
    let tag_offset = reader.offset();
    let payload = match reader.read_u16()? {
        1 => AppEventPayload::DomainEvent(read_delivery(reader, limits)?),
        2 => AppEventPayload::SubscriptionGap {
            subscription_id: read_id(reader, SubscriptionId::new)?,
            gap: read_gap(reader)?,
        },
        3 => AppEventPayload::Backpressure(read_backpressure(reader, limits)?),
        4 => AppEventPayload::ArtifactMetadata(read_artifact_metadata(reader, limits)?),
        5 => AppEventPayload::ArtifactChunk(read_artifact_chunk(reader, limits)?),
        6 => AppEventPayload::ArtifactComplete(read_artifact_completion(reader)?),
        7 => AppEventPayload::PromptRequested(read_prompt_binding(reader, limits)?),
        8 => AppEventPayload::TerminalOutput(read_terminal_output(reader, limits)?),
        9 => AppEventPayload::TerminalExited(read_terminal_exit(reader)?),
        10 => AppEventPayload::ReadinessChanged(read_daemon_status(reader, limits)?),
        11 => {
            let diagnostic = reader.read_str()?.to_owned();
            if diagnostic.len() > limits.max_diagnostic_bytes() {
                return Err(CodecError::at(CodecErrorKind::LimitExceeded, tag_offset));
            }
            AppEventPayload::Diagnostic(invalid(
                tag_offset,
                AppDiagnostic::new(diagnostic, limits.max_diagnostic_bytes()),
            )?)
        }
        12 => AppEventPayload::Heartbeat(read_heartbeat(reader, limits)?),
        13 => AppEventPayload::ShutdownProgress(read_shutdown_progress(reader, limits)?),
        14 => AppEventPayload::ShutdownComplete(read_shutdown_complete(reader, limits)?),
        _ => return unknown(tag_offset),
    };
    if let AppEventPayload::PromptRequested(prompt) = &payload
        && prompt.correlation().session_id() != context.session_id()
    {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, tag_offset));
    }
    Ok(AppEventEnvelope::new(context, payload))
}
