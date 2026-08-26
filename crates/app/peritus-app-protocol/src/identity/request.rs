//! Request and relationship identifiers.

use super::OpaqueAppIdentifier;
use peritus_types::IdentifierError;
use vstd::prelude::*;

verus! {

/// Identifies one negotiated application-protocol relationship.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolId(OpaqueAppIdentifier);

impl ProtocolId {
    /// Exact binary representation length.
    pub const LENGTH: usize = 16;

    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] {
        self.0.spec_bytes()
    }

    /// Returns whether this identity satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool {
        super::valid_app_identifier_bytes(self.spec_bytes())
    }

    /// Creates an identity, rejecting the reserved all-zero value.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError::Zero`] when every input byte is zero.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>)
        ensures
            result.is_ok() == super::valid_app_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        self.0.as_bytes()
    }

    /// Consumes the identity and returns its exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16])
        ensures bytes == self.spec_bytes()
    {
        self.0.into_bytes()
    }
}

/// Identifies one application request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(OpaqueAppIdentifier);

impl RequestId {
    /// Exact binary representation length.
    pub const LENGTH: usize = 16;

    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] {
        self.0.spec_bytes()
    }

    /// Returns whether this identity satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool {
        super::valid_app_identifier_bytes(self.spec_bytes())
    }

    /// Creates an identity, rejecting the reserved all-zero value.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError::Zero`] when every input byte is zero.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>)
        ensures
            result.is_ok() == super::valid_app_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        self.0.as_bytes()
    }

    /// Consumes the identity and returns its exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16])
        ensures bytes == self.spec_bytes()
    {
        self.0.into_bytes()
    }
}

/// Correlates related application messages without granting authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CorrelationId(OpaqueAppIdentifier);

impl CorrelationId {
    /// Exact binary representation length.
    pub const LENGTH: usize = 16;

    /// Returns the mathematical byte-array view.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] {
        self.0.spec_bytes()
    }

    /// Returns whether this identity satisfies its nonzero invariant.
    pub closed spec fn is_valid(&self) -> bool {
        super::valid_app_identifier_bytes(self.spec_bytes())
    }

    /// Creates an identity, rejecting the reserved all-zero value.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError::Zero`] when every input byte is zero.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>)
        ensures
            result.is_ok() == super::valid_app_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        self.0.as_bytes()
    }

    /// Consumes the identity and returns its exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16])
        ensures bytes == self.spec_bytes()
    {
        self.0.into_bytes()
    }
}

} // verus!
