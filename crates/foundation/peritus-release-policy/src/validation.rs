//! Shared checked-construction predicates for release-policy values.

use crate::{ConstructionError, ConstructionErrorKind};
use peritus_types::Sha256Digest;

const fn digest_nonzero(digest: Sha256Digest) -> bool {
    digest_bytes_nonzero(digest.as_bytes())
}
#[doc = " Returns whether a fixed-width digest representation contains a nonzero byte."]
pub const fn digest_bytes_nonzero(bytes: &[u8; 32]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0 {
            return true;
        }
        index += 1;
    }
    false
}
#[doc = " Rejects the reserved all-zero digest placeholder."]
pub const fn require_digest(digest: Sha256Digest) -> Result<(), ConstructionError> {
    if digest_nonzero(digest) {
        Ok(())
    } else {
        Err(ConstructionError::new(ConstructionErrorKind::ZeroDigest))
    }
}
#[doc = " Rejects a zero revision used by an identity-bearing input."]
pub const fn require_revision(revision: u64) -> Result<(), ConstructionError> {
    if revision == 0 {
        Err(ConstructionError::new(ConstructionErrorKind::ZeroRevision))
    } else {
        Ok(())
    }
}
