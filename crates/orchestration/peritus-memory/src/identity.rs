//! Caller-supplied fixed-width identities used by memory records and indexes.

use crate::{MemoryError, MemoryErrorKind, MemoryField};
use vstd::prelude::*;

verus! {

/// Identifies one immutable memory lineage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemoryId {
    bytes: [u8; 16],
}

impl MemoryId {
    /// Binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates an identifier, rejecting the all-zero representation.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::ZeroIdentifier`] for all-zero bytes.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MemoryError> {
        if all_zero(&bytes) {
            Err(MemoryError::field(MemoryErrorKind::ZeroIdentifier, MemoryField::MemoryId))
        } else {
            Ok(Self { bytes })
        }
    }

    /// Borrows the exact stable bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { &self.bytes }

    /// Returns the mathematical stable bytes used by identity specifications.
    pub closed spec fn spec_bytes(&self) -> Seq<u8> { self.bytes@ }

    /// Compares stable bytes with an exact executable-to-specification correspondence.
    pub(crate) const fn same_identity(&self, other: &Self) -> (result: bool)
        ensures result == (self.spec_bytes() == other.spec_bytes()),
    {
        reveal(MemoryId::spec_bytes);
        let mut index = 0;
        while index < Self::LENGTH
            invariant
                index <= Self::LENGTH,
                forall |prior: int| 0 <= prior < index ==>
                    self.bytes@[prior] == other.bytes@[prior],
            decreases Self::LENGTH - index,
        {
            if self.bytes[index] != other.bytes[index] {
                assert(self.bytes@[index as int] != other.bytes@[index as int]);
                return false;
            }
            index += 1;
        }
        assert(self.bytes@ == other.bytes@);
        true
    }

    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] { self.bytes }
}

/// Identifies a durable repository scope without borrowing an ambient path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepositoryId {
    bytes: [u8; 16],
}

impl RepositoryId {
    /// Binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates an identifier, rejecting the all-zero representation.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::ZeroIdentifier`] for all-zero bytes.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MemoryError> {
        if all_zero(&bytes) {
            Err(MemoryError::field(
                MemoryErrorKind::ZeroIdentifier,
                MemoryField::RepositoryId,
            ))
        } else {
            Ok(Self { bytes })
        }
    }

    /// Borrows the exact stable bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { &self.bytes }

    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] { self.bytes }
}

/// Stable semantic key for one provider-neutral retrieval feature.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FeatureKey {
    bytes: [u8; 16],
}

impl FeatureKey {
    /// Binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a key, rejecting the all-zero representation.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::ZeroIdentifier`] for all-zero bytes.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MemoryError> {
        if all_zero(&bytes) {
            Err(MemoryError::field(MemoryErrorKind::ZeroIdentifier, MemoryField::FeatureKey))
        } else {
            Ok(Self { bytes })
        }
    }

    /// Borrows the exact stable bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { &self.bytes }

    /// Consumes the key and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] { self.bytes }
}

const fn all_zero(bytes: &[u8; 16]) -> bool {
    let mut index = 0;
    while index < bytes.len()
        invariant index <= bytes.len(),
        decreases bytes.len() - index,
    {
        if bytes[index] != 0 {
            return false;
        }
        index += 1;
    }
    true
}

} // verus!
