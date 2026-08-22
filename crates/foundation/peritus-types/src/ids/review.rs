//! Identifiers for deterministic gates, adversarial review, and approvals.

use super::base::OpaqueIdentifier;
#[cfg(verus_only)]
use super::base::valid_identifier_bytes;
use crate::IdentifierError;
use vstd::prelude::*;

verus! {

/// Identifies one configured deterministic gate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GateId(OpaqueIdentifier);
impl GateId {
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
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>) ensures
            result.is_ok() == valid_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Returns the exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one execution of a deterministic gate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GateExecutionId(OpaqueIdentifier);
impl GateExecutionId {
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
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>) ensures
            result.is_ok() == valid_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Returns the exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one fresh-context review cycle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewCycleId(OpaqueIdentifier);
impl ReviewCycleId {
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
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>) ensures
            result.is_ok() == valid_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Returns the exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one typed, evidence-backed review finding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingId(OpaqueIdentifier);
impl FindingId {
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
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>) ensures
            result.is_ok() == valid_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Returns the exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one request for explicit authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApprovalRequestId(OpaqueIdentifier);
impl ApprovalRequestId {
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
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>) ensures
            result.is_ok() == valid_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Returns the exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

} // verus!
