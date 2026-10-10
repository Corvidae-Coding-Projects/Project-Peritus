//! Caller-supplied stable identities used by context plans and compaction.

use crate::{ContextError, ContextErrorKind};
use core::cmp::Ordering;
use peritus_types::Sha256Digest;

#[doc = " Stable 128-bit identity for one context node."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContextNodeId([u8; 16]);
impl ContextNodeId {
    #[doc = " Creates an identifier, rejecting the reserved all-zero value."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ContextErrorKind::ZeroIdentifier`] for the all-zero value."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ContextError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(ContextError::plain(ContextErrorKind::ZeroIdentifier))
    }
    #[doc = " Borrows the exact identifier bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    #[doc = " Consumes the identity and returns its exact bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    #[doc = " Returns whether every identity byte is equal."]
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        bytes_equal(self.as_bytes(), other.as_bytes())
    }
    pub(crate) fn canonical_order(&self, other: &Self) -> Ordering {
        compare_bytes_from(self.as_bytes(), other.as_bytes(), 0)
    }
}
#[doc = " Content digest identifying one compaction-policy revision."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompactionPolicyId(Sha256Digest);
impl CompactionPolicyId {
    #[doc = " Wraps the caller-computed policy digest without adding authenticity semantics."]
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }
    #[doc = " Returns the exact policy digest."]
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
}
#[doc = " Content digest identifying one immutable context plan."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContextPlanId(Sha256Digest);
impl ContextPlanId {
    #[doc = " Wraps the caller-computed plan digest without adding authenticity semantics."]
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }
    #[doc = " Returns the exact plan digest."]
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
}
fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> bool {
    let mut index = 0;
    while index < N {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
fn compare_bytes_from<const N: usize>(left: &[u8; N], right: &[u8; N], index: usize) -> Ordering {
    if index == N {
        Ordering::Equal
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_bytes_from(left, right, index + 1)
    }
}
