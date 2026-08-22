//! Identifiers for journal, process, artifact, and evidence records.

use super::base::OpaqueIdentifier;
#[cfg(verus_only)]
use super::base::valid_identifier_bytes;
use crate::IdentifierError;
use vstd::prelude::*;

verus! {

/// Identifies one immutable journal event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventId(OpaqueIdentifier);
impl EventId {
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

/// Identifies one owned operating-system process record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProcessId(OpaqueIdentifier);
impl ProcessId {
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

/// Identifies one content-addressed artifact record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactId(OpaqueIdentifier);
impl ArtifactId {
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

/// Identifies one evidence item or evidence manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceId(OpaqueIdentifier);
impl EvidenceId {
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
