//! Identifiers for deterministic gates, adversarial review, and approvals.

use super::base::OpaqueIdentifier;

use crate::IdentifierError;

#[doc = " Identifies one configured deterministic gate."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GateId(OpaqueIdentifier);
impl GateId {
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
#[doc = " Identifies one execution of a deterministic gate."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GateExecutionId(OpaqueIdentifier);
impl GateExecutionId {
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
#[doc = " Identifies one fresh-context review cycle."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewCycleId(OpaqueIdentifier);
impl ReviewCycleId {
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
#[doc = " Identifies one typed, evidence-backed review finding."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingId(OpaqueIdentifier);
impl FindingId {
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
#[doc = " Identifies one request for explicit authority."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApprovalRequestId(OpaqueIdentifier);
impl ApprovalRequestId {
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
