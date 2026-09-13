//! Scheduler-owned stable nonzero identities.

use crate::{SchedulerError, SchedulerErrorKind};

/// Identifies one immutable run-scoped scheduler aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchedulerId([u8; 16]);

impl SchedulerId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Identifies one immutable admitted scheduler work item.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkId([u8; 16]);

impl WorkId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Identifies one registered scheduler worker.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkerId([u8; 16]);

impl WorkerId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Identifies one durable work-attempt reservation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DispatchId([u8; 16]);

impl DispatchId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

fn checked_identity(bytes: [u8; 16]) -> Result<[u8; 16], SchedulerError> {
    if bytes == [0; 16] {
        Err(crate::error::reject(
            SchedulerErrorKind::InvalidInput,
            "all-zero scheduler identity is reserved",
        ))
    } else {
        Ok(bytes)
    }
}
