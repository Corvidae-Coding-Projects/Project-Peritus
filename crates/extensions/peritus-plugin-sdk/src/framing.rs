//! Four-byte big-endian length-delimited JSON framing under one explicit wire policy.

use std::io;

use serde::{Serialize, de::DeserializeOwned};

use crate::{
    JsonBounds, JsonWirePolicy, SdkError, SdkErrorKind,
    payload::{deserialize_with_bounds, validate_document},
};

const HEADER_BYTES: usize = 4;

/// Typed plugin message whose nested payloads participate in frame admission.
pub trait PluginFrame: Serialize + DeserializeOwned {
    /// Validates nested payloads against the frame's selected payload policy.
    #[doc(hidden)]
    fn validate_payloads(&self, bounds: JsonBounds) -> Result<(), SdkError>;
}

/// Serializes one typed message directly into a bounded JSON frame.
///
/// # Errors
///
/// Rejects payload-policy mismatch, serialization failure, or output beyond selected capacity.
pub fn encode_frame<T: PluginFrame>(
    value: &T,
    policy: JsonWirePolicy,
) -> Result<Vec<u8>, SdkError> {
    value.validate_payloads(policy.payload_bounds())?;
    let mut writer = FrameWriter::new(policy.frame_bytes())?;
    let result = serde_json::to_writer(&mut writer, value);
    if writer.exceeded {
        return Err(limit("encoded frame exceeds its byte capacity"));
    }
    result.map_err(|error| {
        SdkError::new(SdkErrorKind::InvalidFrame, "encode plugin frame", error.to_string())
    })?;
    let body_length = writer.body_length();
    if body_length == 0 {
        return Err(frame_error("encoded frame body is empty"));
    }
    validate_document(&writer.bytes[HEADER_BYTES..], policy.frame_bounds())
        .map_err(frame_validation_error)?;
    let length = u32::try_from(body_length)
        .map_err(|_| limit("encoded frame cannot be represented by its header"))?;
    writer.bytes[..HEADER_BYTES].copy_from_slice(&length.to_be_bytes());
    Ok(writer.bytes)
}

/// Decodes exactly one JSON frame under the same frame and payload policy used by its writer.
///
/// # Errors
///
/// Rejects a short header, zero/oversized length, trailing bytes, duplicates, malformed JSON, or
/// nested payloads outside the selected policy.
pub fn decode_frame<T: PluginFrame>(
    frame: &[u8],
    policy: JsonWirePolicy,
) -> Result<T, SdkError> {
    if frame.len() < HEADER_BYTES {
        return Err(frame_error("frame header is truncated"));
    }
    let length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
    if length == 0 || length > policy.frame_bytes() {
        return Err(limit("declared frame length exceeds its byte capacity"));
    }
    let expected = HEADER_BYTES
        .checked_add(length as usize)
        .ok_or_else(|| frame_error("frame length overflowed"))?;
    if frame.len() != expected {
        return Err(frame_error("frame is truncated or contains trailing bytes"));
    }
    let body = &frame[HEADER_BYTES..];
    validate_document(body, policy.frame_bounds()).map_err(frame_validation_error)?;
    let value: T = deserialize_with_bounds(body, policy.payload_bounds()).map_err(|error| {
        SdkError::new(SdkErrorKind::InvalidFrame, "decode plugin frame", error.to_string())
    })?;
    value.validate_payloads(policy.payload_bounds())?;
    Ok(value)
}

struct FrameWriter {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}

impl FrameWriter {
    fn new(maximum: u32) -> Result<Self, SdkError> {
        let maximum = maximum as usize;
        let capacity = HEADER_BYTES
            .checked_add(maximum.min(8 * 1024))
            .ok_or_else(|| limit("frame allocation capacity overflowed"))?;
        let mut bytes = Vec::with_capacity(capacity);
        bytes.resize(HEADER_BYTES, 0);
        Ok(Self { bytes, maximum, exceeded: false })
    }

    fn body_length(&self) -> usize {
        self.bytes.len() - HEADER_BYTES
    }
}

impl io::Write for FrameWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(length) = self.body_length().checked_add(buffer.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("plugin frame length overflowed"));
        };
        if length > self.maximum {
            self.exceeded = true;
            return Err(io::Error::other("plugin frame exceeds its byte capacity"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn frame_validation_error(error: SdkError) -> SdkError {
    if error.kind() == SdkErrorKind::LimitExceeded {
        error
    } else {
        SdkError::new(SdkErrorKind::InvalidFrame, "decode plugin frame", error.to_string())
    }
}

fn frame_error(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::InvalidFrame, "decode plugin frame", detail)
}

fn limit(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::LimitExceeded, "bound plugin frame", detail)
}
