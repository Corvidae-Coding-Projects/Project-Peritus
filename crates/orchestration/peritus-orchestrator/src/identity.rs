//! Stable nonzero E0 aggregate and handoff identities.

use crate::{OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction};

/// Identifies one durable run-scoped E0 orchestrator aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrchestratorId([u8; 16]);

impl OrchestratorId {
    /// Canonical binary length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero value.
    pub fn new(bytes: [u8; 16]) -> Result<Self, OrchestratorError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Identifies one durable role handoff.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HandoffId([u8; 16]);

impl HandoffId {
    /// Canonical binary length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero value.
    pub fn new(bytes: [u8; 16]) -> Result<Self, OrchestratorError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

fn checked_identity(bytes: [u8; 16]) -> Result<[u8; 16], OrchestratorError> {
    if bytes == [0; 16] {
        Err(OrchestratorError::new(
            OrchestratorErrorKind::InvalidInput,
            OrchestratorRecoveryAction::CorrectInput,
            "all-zero orchestrator identity is reserved",
        ))
    } else {
        Ok(bytes)
    }
}
