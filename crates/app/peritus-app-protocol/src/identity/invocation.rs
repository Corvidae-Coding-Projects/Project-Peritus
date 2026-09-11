//! Nominal identity for a sealed model-request invocation, not a transport request.

use super::OpaqueAppIdentifier;
use peritus_types::IdentifierError;

/// Identifies one immutable workbench request incorporation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkbenchInvocationId(OpaqueAppIdentifier);
impl WorkbenchInvocationId {
    /// Validates the nonzero invocation identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        match OpaqueAppIdentifier::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    /// Borrows exact invocation bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
    /// Returns exact invocation bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}
