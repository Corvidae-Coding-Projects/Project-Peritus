//! Shared verified representation used behind each nominal identifier.

use crate::IdentifierError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct OpaqueIdentifier {
    bytes: [u8; 16],
}
impl OpaqueIdentifier {
    pub(super) const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        if bytes[0] != 0
            || bytes[1] != 0
            || bytes[2] != 0
            || bytes[3] != 0
            || bytes[4] != 0
            || bytes[5] != 0
            || bytes[6] != 0
            || bytes[7] != 0
            || bytes[8] != 0
            || bytes[9] != 0
            || bytes[10] != 0
            || bytes[11] != 0
            || bytes[12] != 0
            || bytes[13] != 0
            || bytes[14] != 0
            || bytes[15] != 0
        {
            Ok(Self { bytes })
        } else {
            Err(IdentifierError::Zero)
        }
    }
    pub(super) const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
    pub(super) const fn into_bytes(self) -> [u8; 16] {
        self.bytes
    }
}
