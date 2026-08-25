use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

use crate::{ProtocolError, ProtocolErrorKind, ProtocolLimits};

pub(super) const MAGIC: [u8; 4] = *b"P5EV";
pub(super) const MAX_CANONICAL_EVENT_BYTES: usize = 64 * 1024 * 1024;

pub(super) const fn codec_limits(limits: ProtocolLimits) -> CodecLimits {
    CodecLimits::new(
        MAX_CANONICAL_EVENT_BYTES,
        MAX_CANONICAL_EVENT_BYTES,
        1_024,
        max_usize(limits.max_text_bytes(), 8 * 1024),
        max_usize(
            limits.max_event_bytes(),
            max_usize(limits.max_tool_argument_bytes(), limits.max_extension_bytes()),
        ),
        32,
    )
}

const fn max_usize(left: usize, right: usize) -> usize {
    if left > right { left } else { right }
}

pub(super) fn write_codec(_: peritus_codec::CodecError) -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidLimit,
        "canonical_event",
        "canonical event encoding exceeded its byte bound",
    )
}

pub(super) fn read_codec(_: peritus_codec::CodecError) -> ProtocolError {
    invalid("canonical_event", "canonical event bytes are malformed or incomplete")
}

pub(super) fn invalid(path: &'static str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidEvent, path, detail)
}

pub(super) fn unknown(path: &'static str) -> ProtocolError {
    invalid(path, "canonical event contains an unknown closed tag")
}

pub(super) fn option_u64(reader: &mut CanonicalReader<'_>) -> Result<Option<u64>, ProtocolError> {
    if reader.read_option_tag().map_err(read_codec)? {
        reader.read_u64().map(Some).map_err(read_codec)
    } else {
        Ok(None)
    }
}

pub(super) fn option_u16(reader: &mut CanonicalReader<'_>) -> Result<Option<u16>, ProtocolError> {
    if reader.read_option_tag().map_err(read_codec)? {
        reader.read_u16().map(Some).map_err(read_codec)
    } else {
        Ok(None)
    }
}

pub(super) fn write_option_u64(
    writer: &mut CanonicalWriter,
    value: Option<u64>,
) -> Result<(), ProtocolError> {
    writer.write_option_tag(value.is_some()).map_err(write_codec)?;
    if let Some(value) = value {
        writer.write_u64(value).map_err(write_codec)?;
    }
    Ok(())
}

pub(super) fn write_option_u16(
    writer: &mut CanonicalWriter,
    value: Option<u16>,
) -> Result<(), ProtocolError> {
    writer.write_option_tag(value.is_some()).map_err(write_codec)?;
    if let Some(value) = value {
        writer.write_u16(value).map_err(write_codec)?;
    }
    Ok(())
}
