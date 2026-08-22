//! Identifiers for project and execution-lifecycle aggregates.

use super::base::OpaqueIdentifier;
#[cfg(verus_only)]
use super::base::valid_identifier_bytes;
use crate::IdentifierError;
use vstd::prelude::*;

verus! {

/// Identifies one configured Peritus project.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectId(OpaqueIdentifier);

impl ProjectId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;
    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
    /// Returns whether the identifier satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }
    /// Creates an identifier, rejecting the reserved all-zero value.
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
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the identifier bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one immutable acceptance specification revision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AcceptanceSpecId(OpaqueIdentifier);

impl AcceptanceSpecId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;
    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
    /// Returns whether the identifier satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }
    /// Creates an identifier, rejecting the reserved all-zero value.
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
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the identifier bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one immutable harness definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HarnessId(OpaqueIdentifier);

impl HarnessId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;
    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
    /// Returns whether the identifier satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }
    /// Creates an identifier, rejecting the reserved all-zero value.
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
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the identifier bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one durable user session.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionId(OpaqueIdentifier);

impl SessionId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;
    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
    /// Returns whether the identifier satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }
    /// Creates an identifier, rejecting the reserved all-zero value.
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
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the identifier bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one governed coding run.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RunId(OpaqueIdentifier);

impl RunId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;
    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
    /// Returns whether the identifier satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }
    /// Creates an identifier, rejecting the reserved all-zero value.
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
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the identifier bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

/// Identifies one attempt within a run.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AttemptId(OpaqueIdentifier);

impl AttemptId {
    /// The binary representation length.
    pub const LENGTH: usize = 16;
    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
    /// Returns whether the identifier satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool { valid_identifier_bytes(self.spec_bytes()) }
    /// Creates an identifier, rejecting the reserved all-zero value.
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
    { match OpaqueIdentifier::new(bytes) { Ok(value) => Ok(Self(value)), Err(error) => Err(error) } }
    /// Borrows the identifier bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16]) ensures *bytes == self.spec_bytes() { self.0.as_bytes() }
    /// Consumes the identifier and returns its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16]) ensures bytes == self.spec_bytes() { self.0.into_bytes() }
}

} // verus!
