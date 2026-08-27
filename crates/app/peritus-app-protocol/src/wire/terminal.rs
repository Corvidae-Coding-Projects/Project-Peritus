//! Canonical terminal attachment value helpers.

use crate::{
    AppProtocolLimits, CorrelationId, RequestId, TerminalAttachmentId, TerminalBinding,
    TerminalCancellation, TerminalDetach, TerminalExit, TerminalExitDisposition, TerminalInput,
    TerminalOutput, TerminalResize, TerminalStream,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::ProcessId;

use super::primitive::{invalid, read_id, unknown, write_id};

pub(super) fn write_terminal_binding(
    writer: &mut CanonicalWriter,
    value: TerminalBinding,
) -> Result<(), CodecError> {
    write_id(writer, value.attachment_id().as_bytes())?;
    write_id(writer, value.process_id().as_bytes())?;
    write_id(writer, value.originating_request_id().as_bytes())
}

pub(super) fn read_terminal_binding(
    reader: &mut CanonicalReader<'_>,
) -> Result<TerminalBinding, CodecError> {
    Ok(TerminalBinding::new(
        read_id(reader, TerminalAttachmentId::new)?,
        read_id(reader, ProcessId::new)?,
        read_id(reader, RequestId::new)?,
    ))
}

pub(super) fn write_terminal_output(
    writer: &mut CanonicalWriter,
    value: &TerminalOutput,
) -> Result<(), CodecError> {
    write_terminal_binding(writer, value.binding())?;
    writer.write_u64(value.sequence())?;
    writer.write_u64(value.offset())?;
    writer.write_u8(match value.stream() {
        TerminalStream::Stdout => 1,
        TerminalStream::Stderr => 2,
        TerminalStream::Terminal => 3,
    })?;
    writer.write_bytes(value.bytes())
}

pub(super) fn read_terminal_output(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<TerminalOutput, CodecError> {
    let offset = reader.offset();
    let binding = read_terminal_binding(reader)?;
    let sequence = reader.read_u64()?;
    let byte_offset = reader.read_u64()?;
    let stream_offset = reader.offset();
    let stream = match reader.read_u8()? {
        1 => TerminalStream::Stdout,
        2 => TerminalStream::Stderr,
        3 => TerminalStream::Terminal,
        _ => return unknown(stream_offset),
    };
    let bytes = reader.read_bytes_owned()?;
    if bytes.len() > limits.max_terminal_chunk_bytes() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(
        offset,
        TerminalOutput::new(
            binding,
            sequence,
            byte_offset,
            stream,
            bytes,
            limits.max_terminal_chunk_bytes(),
        ),
    )
}

pub(super) fn write_terminal_input(
    writer: &mut CanonicalWriter,
    value: &TerminalInput,
) -> Result<(), CodecError> {
    write_terminal_binding(writer, value.binding())?;
    writer.write_bytes(value.bytes())
}

pub(super) fn read_terminal_input(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<TerminalInput, CodecError> {
    let offset = reader.offset();
    let binding = read_terminal_binding(reader)?;
    let bytes = reader.read_bytes_owned()?;
    if bytes.len() > limits.max_terminal_chunk_bytes() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(offset, TerminalInput::new(binding, bytes, limits.max_terminal_chunk_bytes()))
}

pub(super) fn write_terminal_resize(
    writer: &mut CanonicalWriter,
    value: TerminalResize,
) -> Result<(), CodecError> {
    write_terminal_binding(writer, value.binding())?;
    writer.write_u16(value.columns())?;
    writer.write_u16(value.rows())
}

pub(super) fn read_terminal_resize(
    reader: &mut CanonicalReader<'_>,
) -> Result<TerminalResize, CodecError> {
    let offset = reader.offset();
    let binding = read_terminal_binding(reader)?;
    let columns = reader.read_u16()?;
    let rows = reader.read_u16()?;
    invalid(offset, TerminalResize::new(binding, columns, rows, u16::MAX, u16::MAX))
}

pub(super) fn write_terminal_detach(
    writer: &mut CanonicalWriter,
    value: TerminalDetach,
) -> Result<(), CodecError> {
    write_terminal_binding(writer, value.binding())?;
    write_id(writer, value.correlation_id().as_bytes())
}

pub(super) fn read_terminal_detach(
    reader: &mut CanonicalReader<'_>,
) -> Result<TerminalDetach, CodecError> {
    Ok(TerminalDetach::new(read_terminal_binding(reader)?, read_id(reader, CorrelationId::new)?))
}

pub(super) fn write_terminal_cancellation(
    writer: &mut CanonicalWriter,
    value: TerminalCancellation,
) -> Result<(), CodecError> {
    write_terminal_binding(writer, value.binding())?;
    write_id(writer, value.correlation_id().as_bytes())
}

pub(super) fn read_terminal_cancellation(
    reader: &mut CanonicalReader<'_>,
) -> Result<TerminalCancellation, CodecError> {
    Ok(TerminalCancellation::new(
        read_terminal_binding(reader)?,
        read_id(reader, CorrelationId::new)?,
    ))
}

pub(super) fn write_terminal_exit(
    writer: &mut CanonicalWriter,
    value: TerminalExit,
) -> Result<(), CodecError> {
    write_terminal_binding(writer, value.binding())?;
    writer.write_u64(value.next_sequence())?;
    writer.write_u64(value.final_offset())?;
    match value.disposition() {
        TerminalExitDisposition::Code(code) => {
            writer.write_u8(1)?;
            writer.write_fixed(&code.to_be_bytes())
        }
        TerminalExitDisposition::Signal(signal) => {
            writer.write_u8(2)?;
            writer.write_fixed(&signal.to_be_bytes())
        }
        TerminalExitDisposition::Unknown => writer.write_u8(3),
    }
}

pub(super) fn read_terminal_exit(
    reader: &mut CanonicalReader<'_>,
) -> Result<TerminalExit, CodecError> {
    let binding = read_terminal_binding(reader)?;
    let sequence = reader.read_u64()?;
    let final_offset = reader.read_u64()?;
    let tag_offset = reader.offset();
    let disposition = match reader.read_u8()? {
        1 => TerminalExitDisposition::Code(i32::from_be_bytes(reader.read_fixed()?)),
        2 => TerminalExitDisposition::Signal(i32::from_be_bytes(reader.read_fixed()?)),
        3 => TerminalExitDisposition::Unknown,
        _ => return unknown(tag_offset),
    };
    Ok(TerminalExit::new(binding, sequence, final_offset, disposition))
}
