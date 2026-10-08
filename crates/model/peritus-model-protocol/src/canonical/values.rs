//! Canonical scalar encodings shared by semantic request fields.

use super::CanonicalSink;
use crate::ProtocolError;

pub(super) fn optional_text<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: Option<&str>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        text(writer, value)?;
    }
    Ok(())
}

pub(super) fn optional_digest<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: Option<peritus_types::Sha256Digest>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        write_fixed(writer, value.as_bytes())?;
    }
    Ok(())
}

pub(super) fn optional_u64<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: Option<u64>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        u64_value(writer, value)?;
    }
    Ok(())
}

pub(super) fn optional_i64<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: Option<i64>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        write_fixed(writer, &value.to_be_bytes())?;
    }
    Ok(())
}

pub(super) fn optional_u32<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: Option<u32>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        u32_value(writer, value)?;
    }
    Ok(())
}

pub(super) fn collection<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: usize,
) -> Result<(), ProtocolError> {
    writer.write_collection_len(value)
}

pub(super) fn text<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: &str,
) -> Result<(), ProtocolError> {
    writer.write_str(value)
}

pub(super) fn bytes<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: &[u8],
) -> Result<(), ProtocolError> {
    writer.write_bytes(value)
}

pub(super) fn write_fixed<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: &[u8],
) -> Result<(), ProtocolError> {
    writer.write_fixed(value)
}

pub(super) fn boolean<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: bool,
) -> Result<(), ProtocolError> {
    writer.write_fixed(&[u8::from(value)])
}

pub(super) fn option_tag<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    present: bool,
) -> Result<(), ProtocolError> {
    writer.write_fixed(&[u8::from(present)])
}

pub(super) fn u8_value<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: u8,
) -> Result<(), ProtocolError> {
    writer.write_fixed(&[value])
}

pub(super) fn u16_value<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: u16,
) -> Result<(), ProtocolError> {
    writer.write_fixed(&value.to_be_bytes())
}

pub(super) fn u32_value<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: u32,
) -> Result<(), ProtocolError> {
    writer.write_fixed(&value.to_be_bytes())
}

pub(super) fn u64_value<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    value: u64,
) -> Result<(), ProtocolError> {
    writer.write_fixed(&value.to_be_bytes())
}
