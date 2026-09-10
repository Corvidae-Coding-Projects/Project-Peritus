//! Bounded durable control encoding shared by records, operations and receipts.
use super::ControlError;
use crate::control::MAX_CONTROL_BYTES;
use serde::Deserialize;
use serde::Serialize;

pub(super) fn encode(value: &impl Serialize) -> Result<Vec<u8>, ControlError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ControlError::InvalidInput)?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ControlError::Capacity);
    }
    Ok(bytes)
}
pub(super) fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, ControlError> {
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ControlError::Capacity);
    }
    serde_json::from_slice(bytes).map_err(|_| ControlError::InvalidInput)
}
