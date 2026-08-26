//! Canonical exact command binding and final-result helpers.

use crate::{
    AppProtocolLimits, CommandBinding, CommandDisposition, CommandResult, CommandSubmissionFrames,
    CommittedEventRange, CorrelationId, EventCursor, IdempotencyKey, RequestId,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{ActorId, SessionId};

use super::{
    error::{read_app_error, write_app_error},
    primitive::{invalid, read_id, read_option_revision, unknown, write_id, write_option_revision},
};

pub(super) fn write_command_binding(
    writer: &mut CanonicalWriter,
    value: &CommandBinding,
) -> Result<(), CodecError> {
    write_id(writer, value.actor_id().as_bytes())?;
    write_id(writer, value.session_id().as_bytes())?;
    write_id(writer, value.request_id().as_bytes())?;
    write_id(writer, value.correlation_id().as_bytes())?;
    writer.write_bytes(value.idempotency_key().as_bytes())?;
    writer.write_fixed(value.request_digest().as_bytes())?;
    write_option_revision(writer, value.expected_revision())?;
    writer.write_bytes(value.frames().envelope_frame().bytes())?;
    writer.write_bytes(value.frames().command_frame().bytes())
}

pub(super) fn read_command_binding(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<CommandBinding, CodecError> {
    let offset = reader.offset();
    let actor_id = read_id(reader, ActorId::new)?;
    let session_id = read_id(reader, SessionId::new)?;
    let request_id = read_id(reader, RequestId::new)?;
    let correlation_id = read_id(reader, CorrelationId::new)?;
    let key = invalid(reader.offset(), IdempotencyKey::new(reader.read_bytes_owned()?))?;
    let asserted_digest: [u8; 32] = reader.read_fixed()?;
    let revision = read_option_revision(reader)?;
    let envelope = reader.read_bytes_owned()?;
    let command = reader.read_bytes_owned()?;
    let frames = invalid(offset, CommandSubmissionFrames::parse(envelope, command, limits))?;
    let binding = invalid(
        offset,
        CommandBinding::new(
            actor_id,
            session_id,
            request_id,
            correlation_id,
            key,
            revision,
            frames,
        ),
    )?;
    if binding.request_digest().as_bytes() != &asserted_digest {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(binding)
}

pub(super) fn write_command_result(
    writer: &mut CanonicalWriter,
    value: &CommandResult,
) -> Result<(), CodecError> {
    write_id(writer, value.original_request_id().as_bytes())?;
    writer.write_u8(value.disposition().tag())?;
    match value.disposition() {
        CommandDisposition::Committed | CommandDisposition::Replayed => {
            let range = value
                .committed_events()
                .ok_or_else(|| CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len()))?;
            write_range(writer, range)
        }
        CommandDisposition::Rejected => {
            let error = value
                .error()
                .ok_or_else(|| CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len()))?;
            write_app_error(writer, error)
        }
    }
}

pub(super) fn read_command_result(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<CommandResult, CodecError> {
    let request_id = read_id(reader, RequestId::new)?;
    let offset = reader.offset();
    match CommandDisposition::from_tag(reader.read_u8()?) {
        Some(CommandDisposition::Committed) => {
            Ok(CommandResult::committed(request_id, read_range(reader)?))
        }
        Some(CommandDisposition::Replayed) => {
            Ok(CommandResult::replayed(request_id, read_range(reader)?))
        }
        Some(CommandDisposition::Rejected) => {
            Ok(CommandResult::rejected(request_id, read_app_error(reader, limits)?))
        }
        None => unknown(offset),
    }
}

fn write_range(writer: &mut CanonicalWriter, value: CommittedEventRange) -> Result<(), CodecError> {
    writer.write_u64(value.first().get())?;
    writer.write_u64(value.last().get())
}

fn read_range(reader: &mut CanonicalReader<'_>) -> Result<CommittedEventRange, CodecError> {
    let offset = reader.offset();
    let first = EventCursor::new(reader.read_u64()?);
    let last = EventCursor::new(reader.read_u64()?);
    invalid(offset, CommittedEventRange::new(first, last))
}
