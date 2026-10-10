//! Caller-supplied fixed-width identities used by memory records and indexes.

use crate::{MemoryError, MemoryErrorKind, MemoryField};

#[doc = " Identifies one immutable memory lineage."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemoryId {
    bytes: [u8; 16],
}
impl MemoryId {
    #[doc = " Binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting the all-zero representation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`MemoryErrorKind::ZeroIdentifier`] for all-zero bytes."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MemoryError> {
        if all_zero(&bytes) {
            Err(MemoryError::field(MemoryErrorKind::ZeroIdentifier, MemoryField::MemoryId))
        } else {
            Ok(Self { bytes })
        }
    }
    #[doc = " Borrows the exact stable bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
    #[doc = " Compares stable bytes with an exact executable-to-specification correspondence."]
    pub(crate) const fn same_identity(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < Self::LENGTH {
            if self.bytes[index] != other.bytes[index] {
                return false;
            }
            index += 1;
        }
        true
    }
    #[doc = " Consumes the identifier and returns its bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.bytes
    }
}
#[doc = " Identifies a durable repository scope without borrowing an ambient path."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepositoryId {
    bytes: [u8; 16],
}
impl RepositoryId {
    #[doc = " Binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting the all-zero representation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`MemoryErrorKind::ZeroIdentifier`] for all-zero bytes."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MemoryError> {
        if all_zero(&bytes) {
            Err(MemoryError::field(MemoryErrorKind::ZeroIdentifier, MemoryField::RepositoryId))
        } else {
            Ok(Self { bytes })
        }
    }
    #[doc = " Borrows the exact stable bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
    #[doc = " Consumes the identifier and returns its bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.bytes
    }
}
#[doc = " Stable semantic key for one provider-neutral retrieval feature."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FeatureKey {
    bytes: [u8; 16],
}
impl FeatureKey {
    #[doc = " Binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates a key, rejecting the all-zero representation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`MemoryErrorKind::ZeroIdentifier`] for all-zero bytes."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MemoryError> {
        if all_zero(&bytes) {
            Err(MemoryError::field(MemoryErrorKind::ZeroIdentifier, MemoryField::FeatureKey))
        } else {
            Ok(Self { bytes })
        }
    }
    #[doc = " Borrows the exact stable bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
    #[doc = " Consumes the key and returns its bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.bytes
    }
}
const fn all_zero(bytes: &[u8; 16]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0 {
            return false;
        }
        index += 1;
    }
    true
}
