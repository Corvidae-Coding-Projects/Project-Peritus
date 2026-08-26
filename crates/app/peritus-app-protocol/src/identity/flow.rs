//! Subscription, artifact, and prompt identifiers.

use super::OpaqueAppIdentifier;
use peritus_types::IdentifierError;
use vstd::prelude::*;

verus! {

/// Identifies one event subscription.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SubscriptionId(OpaqueAppIdentifier);

impl SubscriptionId {
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

/// Identifies one artifact transfer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransferId(OpaqueAppIdentifier);

impl TransferId {
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

/// Identifies one interactive prompt.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PromptId(OpaqueAppIdentifier);

impl PromptId {
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
