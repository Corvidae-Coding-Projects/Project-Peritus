//! Identifiers for executable work, actors, policy, and workspaces.

use super::base::OpaqueIdentifier;
#[cfg(verus_only)]
use super::base::valid_identifier_bytes;
use crate::IdentifierError;
use vstd::prelude::*;

verus! {

/// Identifies one model interaction turn.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TurnId(OpaqueIdentifier);
impl TurnId {
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

/// Identifies one proposed or executed action.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionId(OpaqueIdentifier);
impl ActionId {
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

/// Identifies one isolated workspace lineage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceId(OpaqueIdentifier);
impl WorkspaceId {
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

/// Identifies one recorded workspace snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SnapshotId(OpaqueIdentifier);
impl SnapshotId {
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

/// Identifies a human, agent, service, or other acting principal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActorId(OpaqueIdentifier);
impl ActorId {
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

/// Identifies one execution environment.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentId(OpaqueIdentifier);
impl EnvironmentId {
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

/// Identifies one capability-addressable resource.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceId(OpaqueIdentifier);
impl ResourceId {
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

/// Identifies one immutable policy definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PolicyId(OpaqueIdentifier);
impl PolicyId {
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

/// Identifies one immutable provider execution profile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderProfileId(OpaqueIdentifier);
impl ProviderProfileId {
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

/// Identifies one submitted domain command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommandId(OpaqueIdentifier);
impl CommandId {
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
