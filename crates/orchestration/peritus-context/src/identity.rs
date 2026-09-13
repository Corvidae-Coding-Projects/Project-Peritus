//! Caller-supplied stable identities used by context plans and compaction.

use crate::{ContextError, ContextErrorKind};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Stable 128-bit identity for one context node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContextNodeId([u8; 16]);

impl ContextNodeId {
    /// Creates an identifier, rejecting the reserved all-zero value.
    ///
    /// # Errors
    ///
    /// Returns [`ContextErrorKind::ZeroIdentifier`] for the all-zero value.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ContextError> {
        let mut index = 0;
        while index < bytes.len()
            decreases bytes.len() - index,
        {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(ContextError::plain(ContextErrorKind::ZeroIdentifier))
    }

    /// Borrows the exact identifier bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { &self.0 }

    /// Consumes the identity and returns its exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] { self.0 }
}

/// Content digest identifying one compaction-policy revision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompactionPolicyId(Sha256Digest);

impl CompactionPolicyId {
    /// Wraps the caller-computed policy digest without adding authenticity semantics.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self { Self(digest) }

    /// Returns the exact policy digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest { self.0 }
}

/// Content digest identifying one immutable context plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContextPlanId(Sha256Digest);

impl ContextPlanId {
    /// Wraps the caller-computed plan digest without adding authenticity semantics.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self { Self(digest) }

    /// Returns the exact plan digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest { self.0 }
}

} // verus!
