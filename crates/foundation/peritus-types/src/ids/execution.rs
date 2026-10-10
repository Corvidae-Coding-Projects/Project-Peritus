//! Identifiers for executable work, actors, policy, and workspaces.

use super::base::OpaqueIdentifier;

use crate::IdentifierError;

#[doc = " Identifies one model interaction turn."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TurnId(OpaqueIdentifier);
impl TurnId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one proposed or executed action."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionId(OpaqueIdentifier);
impl ActionId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one isolated workspace lineage."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceId(OpaqueIdentifier);
impl WorkspaceId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one recorded workspace snapshot."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SnapshotId(OpaqueIdentifier);
impl SnapshotId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies a human, agent, service, or other acting principal."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActorId(OpaqueIdentifier);
impl ActorId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one execution environment."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentId(OpaqueIdentifier);
impl EnvironmentId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one capability-addressable resource."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceId(OpaqueIdentifier);
impl ResourceId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one immutable policy definition."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PolicyId(OpaqueIdentifier);
impl PolicyId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one immutable provider execution profile."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderProfileId(OpaqueIdentifier);
impl ProviderProfileId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one submitted domain command."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommandId(OpaqueIdentifier);
impl CommandId {
    #[doc = " The binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identifier, rejecting all-zero bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Returns the exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
