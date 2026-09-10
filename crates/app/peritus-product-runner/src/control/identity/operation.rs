//! Reconnect-stable idempotency identity for one exact control operation.

use crate::control::ControlError;
use serde::Deserialize;
use serde::Serialize;

/// Reconnect-stable idempotency identity for one exact control operation.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OperationId([u8; 16]);

impl OperationId {
    /// Constructs an identity from nonzero exact bytes.
    ///
    /// # Errors
    /// Rejects the reserved all-zero value.
    pub fn new(bytes: [u8; 16]) -> Result<Self, ControlError> {
        if bytes == [0; 16] { Err(ControlError::InvalidInput) } else { Ok(Self(bytes)) }
    }

    /// Borrows the exact stable identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl TryFrom<[u8; 16]> for OperationId {
    type Error = ControlError;

    fn try_from(bytes: [u8; 16]) -> Result<Self, Self::Error> {
        Self::new(bytes)
    }
}

impl<'de> Deserialize<'de> for OperationId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 16]>::deserialize(deserializer)?;
        Self::new(bytes).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OperationId({self})")
    }
}
