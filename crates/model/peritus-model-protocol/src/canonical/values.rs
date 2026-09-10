//! Canonical scalar encodings shared by semantic request fields.

use crate::{ProtocolError, ProtocolErrorKind};
use peritus_codec::CanonicalWriter;

pub(super) fn optional_text(
    writer: &mut CanonicalWriter,
    value: Option<&str>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        text(writer, value)?;
    }
    Ok(())
}

pub(super) fn optional_digest(
    writer: &mut CanonicalWriter,
    value: Option<peritus_types::Sha256Digest>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        write_fixed(writer, value.as_bytes())?;
    }
    Ok(())
}

pub(super) fn optional_u64(
    writer: &mut CanonicalWriter,
    value: Option<u64>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        u64_value(writer, value)?;
    }
    Ok(())
}

pub(super) fn optional_i64(
    writer: &mut CanonicalWriter,
    value: Option<i64>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        write_fixed(writer, &value.to_be_bytes())?;
    }
    Ok(())
}

pub(super) fn optional_u32(
    writer: &mut CanonicalWriter,
    value: Option<u32>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        u32_value(writer, value)?;
    }
    Ok(())
}

pub(super) fn collection(writer: &mut CanonicalWriter, value: usize) -> Result<(), ProtocolError> {
    writer.write_collection_len(value).map_err(codec)
}

pub(super) fn text(writer: &mut CanonicalWriter, value: &str) -> Result<(), ProtocolError> {
    writer.write_str(value).map_err(codec)
}

pub(super) fn bytes(writer: &mut CanonicalWriter, value: &[u8]) -> Result<(), ProtocolError> {
    writer.write_bytes(value).map_err(codec)
}

pub(super) fn write_fixed(writer: &mut CanonicalWriter, value: &[u8]) -> Result<(), ProtocolError> {
    writer.write_fixed(value).map_err(codec)
}

pub(super) fn boolean(writer: &mut CanonicalWriter, value: bool) -> Result<(), ProtocolError> {
    writer.write_bool(value).map_err(codec)
}

pub(super) fn option_tag(writer: &mut CanonicalWriter, present: bool) -> Result<(), ProtocolError> {
    writer.write_option_tag(present).map_err(codec)
}

pub(super) fn u8_value(writer: &mut CanonicalWriter, value: u8) -> Result<(), ProtocolError> {
    writer.write_u8(value).map_err(codec)
}

pub(super) fn u16_value(writer: &mut CanonicalWriter, value: u16) -> Result<(), ProtocolError> {
    writer.write_u16(value).map_err(codec)
}

pub(super) fn u32_value(writer: &mut CanonicalWriter, value: u32) -> Result<(), ProtocolError> {
    writer.write_u32(value).map_err(codec)
}

pub(super) fn u64_value(writer: &mut CanonicalWriter, value: u64) -> Result<(), ProtocolError> {
    writer.write_u64(value).map_err(codec)
}

fn codec(_: peritus_codec::CodecError) -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidLimit,
        "canonical_request",
        "canonical request encoding exceeded an internal bound",
    )
}
