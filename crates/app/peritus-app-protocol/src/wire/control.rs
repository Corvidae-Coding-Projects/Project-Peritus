//! Canonical schema-v1 application control family.

use crate::{
    APP_SCHEMA_V1, AppProtocolLimits, CONTROL_FAMILY, ControlEnvelope, ControlPayload,
    CorrelationId, HeartbeatId, HeartbeatReply, SubscriptionControl, SubscriptionId,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use super::{
    artifact::{read_artifact_cancellation, write_artifact_cancellation},
    primitive::{read_context, read_id, unknown, write_context, write_id},
    prompt::{read_prompt_cancellation, write_prompt_cancellation},
    subscription::{
        read_acknowledgement, read_pause_reason, read_subscription_cancellation,
        write_acknowledgement, write_pause_reason, write_subscription_cancellation,
    },
    terminal::{read_terminal_cancellation, write_terminal_cancellation},
};

impl CanonicalEncode for ControlEnvelope {
    const FAMILY: u16 = CONTROL_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        validate_control_binding(self, writer.len())?;
        write_context(writer, self.context())?;
        write_id(writer, self.correlation_id().as_bytes())?;
        match self.payload() {
            ControlPayload::Acknowledge(value) => {
                writer.write_u16(1)?;
                write_acknowledgement(writer, *value)
            }
            ControlPayload::CancelSubscription(value) => {
                writer.write_u16(2)?;
                write_subscription_cancellation(writer, *value)
            }
            ControlPayload::CancelArtifact(value) => {
                writer.write_u16(3)?;
                write_artifact_cancellation(writer, *value)
            }
            ControlPayload::CancelPrompt(value) => {
                writer.write_u16(4)?;
                write_prompt_cancellation(writer, *value)
            }
            ControlPayload::CancelTerminal(value) => {
                writer.write_u16(5)?;
                write_terminal_cancellation(writer, *value)
            }
            ControlPayload::Subscription(value) => {
                writer.write_u16(6)?;
                write_subscription_control(writer, *value)
            }
            ControlPayload::HeartbeatReply(value) => {
                writer.write_u16(7)?;
                write_heartbeat_reply(writer, *value)
            }
        }
    }
}

impl CanonicalDecode for ControlEnvelope {
    const FAMILY: u16 = CONTROL_FAMILY;
    const SCHEMA_VERSION: u16 = APP_SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_control(reader, AppProtocolLimits::PRODUCTION)
    }
}

pub(super) fn read_control(
    reader: &mut CanonicalReader<'_>,
    _limits: AppProtocolLimits,
) -> Result<ControlEnvelope, CodecError> {
    let offset = reader.offset();
    let context = read_context(reader)?;
    let correlation_id = read_id(reader, CorrelationId::new)?;
    let tag_offset = reader.offset();
    let payload = match reader.read_u16()? {
        1 => ControlPayload::Acknowledge(read_acknowledgement(reader)?),
        2 => ControlPayload::CancelSubscription(read_subscription_cancellation(reader)?),
        3 => ControlPayload::CancelArtifact(read_artifact_cancellation(reader)?),
        4 => ControlPayload::CancelPrompt(read_prompt_cancellation(reader)?),
        5 => ControlPayload::CancelTerminal(read_terminal_cancellation(reader)?),
        6 => ControlPayload::Subscription(read_subscription_control(reader)?),
        7 => ControlPayload::HeartbeatReply(read_heartbeat_reply(reader)?),
        _ => return unknown(tag_offset),
    };
    let control = ControlEnvelope::new(context, correlation_id, payload);
    validate_control_binding(&control, offset)?;
    Ok(control)
}

fn write_subscription_control(
    writer: &mut CanonicalWriter,
    value: SubscriptionControl,
) -> Result<(), CodecError> {
    match value {
        SubscriptionControl::Pause { subscription_id, reason } => {
            writer.write_u8(1)?;
            write_id(writer, subscription_id.as_bytes())?;
            write_pause_reason(writer, reason)
        }
        SubscriptionControl::Resume { subscription_id } => {
            writer.write_u8(2)?;
            write_id(writer, subscription_id.as_bytes())
        }
    }
}

fn read_subscription_control(
    reader: &mut CanonicalReader<'_>,
) -> Result<SubscriptionControl, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(SubscriptionControl::Pause {
            subscription_id: read_id(reader, SubscriptionId::new)?,
            reason: read_pause_reason(reader)?,
        }),
        2 => Ok(SubscriptionControl::Resume {
            subscription_id: read_id(reader, SubscriptionId::new)?,
        }),
        _ => unknown(offset),
    }
}

fn write_heartbeat_reply(
    writer: &mut CanonicalWriter,
    value: HeartbeatReply,
) -> Result<(), CodecError> {
    write_id(writer, value.heartbeat_id().as_bytes())?;
    writer.write_u64(value.sequence())
}

fn read_heartbeat_reply(reader: &mut CanonicalReader<'_>) -> Result<HeartbeatReply, CodecError> {
    Ok(HeartbeatReply::new(read_id(reader, HeartbeatId::new)?, reader.read_u64()?))
}

fn validate_control_binding(value: &ControlEnvelope, offset: usize) -> Result<(), CodecError> {
    let matches = match value.payload() {
        ControlPayload::CancelSubscription(inner) => {
            inner.correlation_id() == value.correlation_id()
        }
        ControlPayload::CancelArtifact(inner) => inner.correlation_id() == value.correlation_id(),
        ControlPayload::CancelPrompt(inner) => {
            if inner.correlation_id() == value.correlation_id() {
                inner.correlation().session_id() == value.context().session_id()
            } else {
                false
            }
        }
        ControlPayload::CancelTerminal(inner) => inner.correlation_id() == value.correlation_id(),
        _ => true,
    };
    if matches { Ok(()) } else { Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset)) }
}
