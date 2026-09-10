//! Stable user conversation identity, distinct from a run or transport session.

use crate::control::ControlError;
use serde::Deserialize;
use serde::Serialize;

/// Stable user conversation identity, distinct from a run or transport session.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ConversationId([u8; 16]);

impl ConversationId {
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

impl TryFrom<[u8; 16]> for ConversationId {
    type Error = ControlError;

    fn try_from(bytes: [u8; 16]) -> Result<Self, Self::Error> {
        Self::new(bytes)
    }
}

impl<'de> Deserialize<'de> for ConversationId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 16]>::deserialize(deserializer)?;
        Self::new(bytes).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConversationId({self})")
    }
}
