//! Identifiers for journal, process, artifact, and evidence records.

use super::base::OpaqueIdentifier;

use crate::IdentifierError;

#[doc = " Identifies one immutable journal event."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventId(OpaqueIdentifier);
impl EventId {
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
#[doc = " Identifies one owned operating-system process record."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProcessId(OpaqueIdentifier);
impl ProcessId {
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
#[doc = " Identifies one content-addressed artifact record."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactId(OpaqueIdentifier);
impl ArtifactId {
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
#[doc = " Identifies one evidence item or evidence manifest."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceId(OpaqueIdentifier);
impl EvidenceId {
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
