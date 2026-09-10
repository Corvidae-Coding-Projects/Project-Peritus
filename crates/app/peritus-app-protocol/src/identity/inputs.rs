//! Nominal workbench input identity, distinct from transport requests and control operations.

use super::OpaqueAppIdentifier;
use peritus_types::IdentifierError;

/// Identifies immutable revisions of one accepted user input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkbenchInputId(OpaqueAppIdentifier);
impl WorkbenchInputId {
    /// Validates the nonzero nominal identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
