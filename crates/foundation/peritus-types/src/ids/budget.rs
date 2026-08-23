//! Identifiers for immutable budget accounts and reservation lineages.

use super::base::OpaqueIdentifier;
#[cfg(verus_only)]
use super::base::valid_identifier_bytes;
use crate::IdentifierError;
use vstd::prelude::*;

verus! {

/// Identifies one immutable hierarchical budget account.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BudgetId(OpaqueIdentifier);

impl BudgetId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;

    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }

    /// Returns whether the identifier is nonzero.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }

    /// Creates an identifier, rejecting all-zero bytes.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError::Zero`] when every input byte is zero.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>)
        ensures
            result.is_ok() == valid_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes(),
    {
        self.0.as_bytes()
    }

    /// Returns the exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16])
        ensures bytes == self.spec_bytes(),
    {
        self.0.into_bytes()
    }
}

/// Identifies one idempotent budget reservation and charge lineage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BudgetReservationId(OpaqueIdentifier);

impl BudgetReservationId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;

    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }

    /// Returns whether the identifier is nonzero.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }

    /// Creates an identifier, rejecting all-zero bytes.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError::Zero`] when every input byte is zero.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>)
        ensures
            result.is_ok() == valid_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes(),
    {
        self.0.as_bytes()
    }

    /// Returns the exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16])
        ensures bytes == self.spec_bytes(),
    {
        self.0.into_bytes()
    }
}

} // verus!
