//! Collaboration-owned checked identities.

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use vstd::prelude::*;

verus! {

/// Returns whether the canonical identity bytes are not the reserved all-zero value.
pub open spec fn valid_identity_bytes(bytes: [u8; 16]) -> bool {
    bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
        || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
        || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
        || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
}

const fn valid(bytes: [u8; 16]) -> (result: bool)
    ensures result == valid_identity_bytes(bytes)
{
    bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
        || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
        || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
        || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
}

} // verus!

/// Identifies one collaboration aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CollaborationId([u8; 16]);

impl CollaborationId {
    /// Canonical byte length.
    pub const LENGTH: usize = 16;

    /// Creates an identity, rejecting all-zero bytes.
    ///
    /// # Errors
    /// Returns [`CollaborationErrorKind::InvalidInput`] for the reserved zero value.
    pub fn new(bytes: [u8; 16]) -> Result<Self, CollaborationError> {
        if valid(bytes) { Ok(Self(bytes)) } else { Err(invalid_identity()) }
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

/// Identifies one causal collaboration task.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CollaborationTaskId([u8; 16]);

impl CollaborationTaskId {
    /// Canonical byte length.
    pub const LENGTH: usize = 16;

    /// Creates an identity, rejecting all-zero bytes.
    ///
    /// # Errors
    /// Returns [`CollaborationErrorKind::InvalidInput`] for the reserved zero value.
    pub fn new(bytes: [u8; 16]) -> Result<Self, CollaborationError> {
        if valid(bytes) { Ok(Self(bytes)) } else { Err(invalid_identity()) }
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

/// Identifies one durable collaboration message.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CollaborationMessageId([u8; 16]);

impl CollaborationMessageId {
    /// Canonical byte length.
    pub const LENGTH: usize = 16;

    /// Creates an identity, rejecting all-zero bytes.
    ///
    /// # Errors
    /// Returns [`CollaborationErrorKind::InvalidInput`] for the reserved zero value.
    pub fn new(bytes: [u8; 16]) -> Result<Self, CollaborationError> {
        if valid(bytes) { Ok(Self(bytes)) } else { Err(invalid_identity()) }
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

fn invalid_identity() -> CollaborationError {
    reject(CollaborationErrorKind::InvalidInput, "all-zero collaboration identity is reserved")
}
