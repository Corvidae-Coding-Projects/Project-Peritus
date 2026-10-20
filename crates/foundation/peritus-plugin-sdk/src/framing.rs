//! Four-byte big-endian length-delimited JSON framing.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{SdkError, SdkErrorKind};

const HEADER_BYTES: usize = 4;

/// Serializes one typed message into a bounded length-delimited JSON frame.
///
/// # Errors
///
/// Rejects serialization failure, a zero limit, or a payload larger than the limit/u32 range.
pub fn encode_frame<T: Serialize>(value: &T, maximum_bytes: u32) -> Result<Vec<u8>, SdkError> {
    if maximum_bytes == 0 {
        return Err(limit("frame limit must be positive"));
    }
    let payload = serde_json::to_vec(value).map_err(|error| {
        SdkError::new(SdkErrorKind::InvalidFrame, "encode plugin frame", error.to_string())
    })?;
    if payload.is_empty() || payload.len() > maximum_bytes as usize {
        return Err(limit("encoded frame exceeds its byte bound"));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| limit("encoded frame cannot be represented by its header"))?;
    let mut frame = Vec::with_capacity(HEADER_BYTES + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decodes exactly one bounded length-delimited JSON frame.
///
/// # Errors
///
/// Rejects a short header, zero/oversized length, trailing bytes, or malformed JSON.
pub fn decode_frame<T: DeserializeOwned>(frame: &[u8], maximum_bytes: u32) -> Result<T, SdkError> {
    if frame.len() < HEADER_BYTES {
        return Err(frame_error("frame header is truncated"));
    }
    let length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
    if length == 0 || length > maximum_bytes {
        return Err(limit("declared frame length exceeds its byte bound"));
    }
    let expected = HEADER_BYTES
        .checked_add(length as usize)
        .ok_or_else(|| frame_error("frame length overflowed"))?;
    if frame.len() != expected {
        return Err(frame_error("frame is truncated or contains trailing bytes"));
    }
    serde_json::from_slice(&frame[HEADER_BYTES..]).map_err(|error| {
        SdkError::new(SdkErrorKind::InvalidFrame, "decode plugin frame", error.to_string())
    })
}

fn frame_error(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::InvalidFrame, "decode plugin frame", detail)
}

fn limit(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::LimitExceeded, "bound plugin frame", detail)
}
