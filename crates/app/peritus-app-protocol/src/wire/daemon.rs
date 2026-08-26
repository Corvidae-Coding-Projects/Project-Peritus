//! Canonical daemon status, heartbeat, and shutdown helpers.

use crate::{
    AppProtocolLimits, CorrelationId, DaemonHeartbeat, DaemonReadiness, DaemonStatus, HeartbeatId,
    RemainingWork, RemainingWorkKind, RequestId, ShutdownAccepted, ShutdownComplete,
    ShutdownCompletionDisposition, ShutdownProgress, ShutdownRequest,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

use super::primitive::{
    invalid, read_id, read_string_option, unknown, write_id, write_string_option,
};

pub(super) fn write_daemon_status(
    writer: &mut CanonicalWriter,
    value: &DaemonStatus,
) -> Result<(), CodecError> {
    writer.write_u8(readiness_tag(value.readiness()))?;
    write_string_option(writer, value.diagnostic())
}

pub(super) fn read_daemon_status(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<DaemonStatus, CodecError> {
    let offset = reader.offset();
    let readiness = read_readiness(reader)?;
    let diagnostic = read_string_option(reader)?;
    if diagnostic.as_ref().is_some_and(|value| value.len() > limits.max_diagnostic_bytes()) {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(offset, DaemonStatus::new(readiness, diagnostic, limits.max_diagnostic_bytes()))
}

pub(super) fn write_heartbeat(
    writer: &mut CanonicalWriter,
    value: &DaemonHeartbeat,
) -> Result<(), CodecError> {
    write_id(writer, value.heartbeat_id().as_bytes())?;
    writer.write_u64(value.sequence())?;
    write_daemon_status(writer, value.status())
}

pub(super) fn read_heartbeat(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<DaemonHeartbeat, CodecError> {
    Ok(DaemonHeartbeat::new(
        read_id(reader, HeartbeatId::new)?,
        reader.read_u64()?,
        read_daemon_status(reader, limits)?,
    ))
}

pub(super) fn write_shutdown_request(
    writer: &mut CanonicalWriter,
    value: ShutdownRequest,
) -> Result<(), CodecError> {
    write_id(writer, value.request_id().as_bytes())?;
    write_id(writer, value.correlation_id().as_bytes())
}

pub(super) fn read_shutdown_request(
    reader: &mut CanonicalReader<'_>,
) -> Result<ShutdownRequest, CodecError> {
    Ok(ShutdownRequest::new(read_id(reader, RequestId::new)?, read_id(reader, CorrelationId::new)?))
}

pub(super) fn write_shutdown_accepted(
    writer: &mut CanonicalWriter,
    value: ShutdownAccepted,
) -> Result<(), CodecError> {
    write_shutdown_request(writer, value.request())
}

pub(super) fn read_shutdown_accepted(
    reader: &mut CanonicalReader<'_>,
) -> Result<ShutdownAccepted, CodecError> {
    Ok(ShutdownAccepted::new(read_shutdown_request(reader)?))
}

pub(super) fn write_shutdown_progress(
    writer: &mut CanonicalWriter,
    value: &ShutdownProgress,
) -> Result<(), CodecError> {
    write_shutdown_request(writer, value.request())?;
    writer.write_u32(value.completed_steps())?;
    writer.write_u32(value.total_steps())?;
    write_remaining(writer, value.remaining())
}

pub(super) fn read_shutdown_progress(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<ShutdownProgress, CodecError> {
    let offset = reader.offset();
    let request = read_shutdown_request(reader)?;
    let completed = reader.read_u32()?;
    let total = reader.read_u32()?;
    let remaining = read_remaining(reader, limits)?;
    invalid(
        offset,
        ShutdownProgress::new(
            request,
            completed,
            total,
            remaining,
            limits.max_remaining_work_items(),
        ),
    )
}

pub(super) fn write_shutdown_complete(
    writer: &mut CanonicalWriter,
    value: &ShutdownComplete,
) -> Result<(), CodecError> {
    write_shutdown_request(writer, value.request())?;
    writer.write_u8(match value.disposition() {
        ShutdownCompletionDisposition::Clean => 1,
        ShutdownCompletionDisposition::Unclean => 2,
    })?;
    write_remaining(writer, value.remaining())
}

pub(super) fn read_shutdown_complete(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<ShutdownComplete, CodecError> {
    let offset = reader.offset();
    let request = read_shutdown_request(reader)?;
    let disposition_offset = reader.offset();
    let disposition = match reader.read_u8()? {
        1 => ShutdownCompletionDisposition::Clean,
        2 => ShutdownCompletionDisposition::Unclean,
        _ => return unknown(disposition_offset),
    };
    let remaining = read_remaining(reader, limits)?;
    invalid(
        offset,
        ShutdownComplete::new(request, disposition, remaining, limits.max_remaining_work_items()),
    )
}

fn write_remaining(
    writer: &mut CanonicalWriter,
    values: &[RemainingWork],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        writer.write_u8(match value.kind() {
            RemainingWorkKind::Request => 1,
            RemainingWorkKind::Subscription => 2,
            RemainingWorkKind::ArtifactTransfer => 3,
            RemainingWorkKind::TerminalAttachment => 4,
            RemainingWorkKind::Other => 5,
        })?;
        writer.write_str(value.descriptor())?;
    }
    Ok(())
}

fn read_remaining(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<Vec<RemainingWork>, CodecError> {
    let offset = reader.offset();
    let length = reader.read_collection_len()?;
    if length > limits.max_remaining_work_items() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        let item_offset = reader.offset();
        let kind = match reader.read_u8()? {
            1 => RemainingWorkKind::Request,
            2 => RemainingWorkKind::Subscription,
            3 => RemainingWorkKind::ArtifactTransfer,
            4 => RemainingWorkKind::TerminalAttachment,
            5 => RemainingWorkKind::Other,
            _ => return unknown(item_offset),
        };
        let descriptor = reader.read_str()?.to_owned();
        if descriptor.len() > limits.max_diagnostic_bytes() {
            return Err(CodecError::at(CodecErrorKind::LimitExceeded, item_offset));
        }
        values.push(invalid(
            item_offset,
            RemainingWork::new(kind, descriptor, limits.max_diagnostic_bytes()),
        )?);
    }
    Ok(values)
}

const fn readiness_tag(value: DaemonReadiness) -> u8 {
    match value {
        DaemonReadiness::Starting => 1,
        DaemonReadiness::ReadyReadWrite => 2,
        DaemonReadiness::ReadyReadOnly => 3,
        DaemonReadiness::Draining => 4,
        DaemonReadiness::Unavailable => 5,
    }
}

fn read_readiness(reader: &mut CanonicalReader<'_>) -> Result<DaemonReadiness, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(DaemonReadiness::Starting),
        2 => Ok(DaemonReadiness::ReadyReadWrite),
        3 => Ok(DaemonReadiness::ReadyReadOnly),
        4 => Ok(DaemonReadiness::Draining),
        5 => Ok(DaemonReadiness::Unavailable),
        _ => unknown(offset),
    }
}
