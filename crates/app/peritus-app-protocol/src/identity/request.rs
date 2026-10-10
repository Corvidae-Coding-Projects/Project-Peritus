//! Request and relationship identifiers.

use super::OpaqueAppIdentifier;
use peritus_types::IdentifierError;

#[doc = " Identifies one negotiated application-protocol relationship."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolId(OpaqueAppIdentifier);
impl ProtocolId {
    #[doc = " Exact binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identity, rejecting the reserved all-zero value."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Consumes the identity and returns its exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Identifies one application request."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(OpaqueAppIdentifier);
impl RequestId {
    #[doc = " Exact binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identity, rejecting the reserved all-zero value."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Consumes the identity and returns its exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
#[doc = " Correlates related application messages without granting authority."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CorrelationId(OpaqueAppIdentifier);
impl CorrelationId {
    #[doc = " Exact binary representation length."]
    pub const LENGTH: usize = 16;
    #[doc = " Creates an identity, rejecting the reserved all-zero value."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`IdentifierError::Zero`] when every input byte is zero."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    #[doc = " Consumes the identity and returns its exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
