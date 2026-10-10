//! Identifiers for immutable budget accounts and reservation lineages.

use super::base::OpaqueIdentifier;

use crate::IdentifierError;

#[doc = " Identifies one immutable hierarchical budget account."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BudgetId(OpaqueIdentifier);
impl BudgetId {
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
#[doc = " Identifies one idempotent budget reservation and charge lineage."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BudgetReservationId(OpaqueIdentifier);
impl BudgetReservationId {
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
