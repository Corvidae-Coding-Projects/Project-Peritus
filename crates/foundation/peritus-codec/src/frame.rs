//! Version-one canonical frame header.

#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "all frame failures are represented by CodecError; fixed-header conversions follow an exact length check"
)]

use crate::{CodecError, CodecErrorKind, CodecLimit, CodecLimits};

/// Fixed frame magic.
pub const MAGIC: [u8; 4] = *b"PRTS";
/// Canonical primitive/framing format version.
pub const FORMAT_VERSION: u16 = 1;
/// Exact fixed header length.
pub const HEADER_LEN: usize = 16;

/// Checked immutable frame header.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrameHeader {
    family: u16,
    schema_version: u16,
    payload_len: u32,
}

impl FrameHeader {
    /// Returns the nonzero message-family tag.
    #[must_use]
    pub const fn family(self) -> u16 {
        self.family
    }
    /// Returns the nonzero family schema version.
    #[must_use]
    pub const fn schema_version(self) -> u16 {
        self.schema_version
    }
    /// Returns the exact declared payload length.
    #[must_use]
    pub const fn payload_len(self) -> u32 {
        self.payload_len
    }
}

/// Borrowed checked canonical frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedFrame<'a> {
    header: FrameHeader,
    payload: &'a [u8],
}

impl<'a> DecodedFrame<'a> {
    /// Returns the checked header.
    #[must_use]
    pub const fn header(self) -> FrameHeader {
        self.header
    }
    /// Returns the exact borrowed payload.
    #[must_use]
    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }
}

/// Encodes one complete canonical frame.
pub fn encode_frame(
    family: u16,
    schema_version: u16,
    payload: &[u8],
    limits: CodecLimits,
) -> Result<Vec<u8>, CodecError> {
    if family == 0 {
        return Err(CodecError::new(CodecErrorKind::InvalidFamily, 6));
    }
    if schema_version == 0 {
        return Err(CodecError::new(CodecErrorKind::InvalidSchemaVersion, 8));
    }
    if payload.len() > limits.max_payload_bytes {
        return Err(CodecError::limited(12, CodecLimit::PayloadBytes));
    }
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, 12))?;
    let frame_len = HEADER_LEN
        .checked_add(payload.len())
        .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, 12))?;
    if frame_len > limits.max_frame_bytes {
        return Err(CodecError::limited(0, CodecLimit::FrameBytes));
    }
    let mut frame = Vec::with_capacity(frame_len);
    frame.extend_from_slice(&MAGIC);
    frame.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    frame.extend_from_slice(&family.to_be_bytes());
    frame.extend_from_slice(&schema_version.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&payload_len.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Decodes and validates one complete canonical frame.
pub fn decode_frame(input: &[u8], limits: CodecLimits) -> Result<DecodedFrame<'_>, CodecError> {
    if input.len() > limits.max_frame_bytes {
        return Err(CodecError::limited(0, CodecLimit::FrameBytes));
    }
    if input.len() < HEADER_LEN {
        return Err(CodecError::new(CodecErrorKind::Truncated, input.len()));
    }
    if input[..4] != MAGIC {
        return Err(CodecError::new(CodecErrorKind::InvalidMagic, 0));
    }
    let format_version = u16::from_be_bytes(input[4..6].try_into().expect("fixed header"));
    if format_version != FORMAT_VERSION {
        return Err(CodecError::new(CodecErrorKind::UnsupportedFormatVersion, 4));
    }
    let family = u16::from_be_bytes(input[6..8].try_into().expect("fixed header"));
    if family == 0 {
        return Err(CodecError::new(CodecErrorKind::InvalidFamily, 6));
    }
    let schema_version = u16::from_be_bytes(input[8..10].try_into().expect("fixed header"));
    if schema_version == 0 {
        return Err(CodecError::new(CodecErrorKind::InvalidSchemaVersion, 8));
    }
    let flags = u16::from_be_bytes(input[10..12].try_into().expect("fixed header"));
    if flags != 0 {
        return Err(CodecError::new(CodecErrorKind::NonzeroFlags, 10));
    }
    let payload_len = u32::from_be_bytes(input[12..16].try_into().expect("fixed header"));
    let payload_usize = usize::try_from(payload_len)
        .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, 12))?;
    if payload_usize > limits.max_payload_bytes {
        return Err(CodecError::limited(12, CodecLimit::PayloadBytes));
    }
    let expected = HEADER_LEN
        .checked_add(payload_usize)
        .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, 12))?;
    if input.len() < expected {
        return Err(CodecError::new(CodecErrorKind::Truncated, input.len()));
    }
    if input.len() > expected {
        return Err(CodecError::new(CodecErrorKind::TrailingBytes, expected));
    }
    Ok(DecodedFrame {
        header: FrameHeader { family, schema_version, payload_len },
        payload: &input[HEADER_LEN..expected],
    })
}
