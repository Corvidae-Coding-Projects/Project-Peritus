//! Caller-supplied stable identities used by context plans and compaction.

use crate::{ContextError, ContextErrorKind};
use core::cmp::Ordering;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Stable 128-bit identity for one context node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContextNodeId([u8; 16]);

impl ContextNodeId {
    /// Logical view of every stable identity byte.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0 }

    /// Exact byte identity equality.
    pub open spec fn spec_matches(&self, other: &Self) -> bool {
        self.spec_bytes() == other.spec_bytes()
    }

    /// Exact canonical lexicographic order.
    pub open spec fn spec_order(&self, other: &Self) -> Ordering {
        peritus_types::canonical_byte_order_from(
            self.spec_bytes()@,
            other.spec_bytes()@,
            0,
        )
    }

    /// Creates an identifier, rejecting the reserved all-zero value.
    ///
    /// # Errors
    ///
    /// Returns [`ContextErrorKind::ZeroIdentifier`] for the all-zero value.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, ContextError>)
        ensures
            result.is_ok() == (exists |index: int| 0 <= index < 16 && bytes[index] != 0),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_is_plain(ContextErrorKind::ZeroIdentifier),
            },
    {
        let mut index = 0;
        while index < bytes.len()
            invariant
                index <= bytes.len(),
                forall |prior: int| 0 <= prior < index ==> bytes[prior] == 0,
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
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes(),
    { &self.0 }

    /// Consumes the identity and returns its exact bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 16])
        ensures bytes == self.spec_bytes(),
    { self.0 }

    /// Returns whether every identity byte is equal.
    #[must_use]
    pub fn matches(&self, other: &Self) -> (matches: bool)
        ensures matches == self.spec_matches(other),
    {
        bytes_equal(self.as_bytes(), other.as_bytes())
    }

    pub(crate) fn canonical_order(&self, other: &Self) -> (order: Ordering)
        ensures order == self.spec_order(other),
    {
        compare_bytes_from(self.as_bytes(), other.as_bytes(), 0)
    }

    pub(crate) proof fn matches_implies_equal(left: &Self, right: &Self)
        requires left.spec_matches(right),
        ensures *left == *right,
    {
        reveal(ContextNodeId::spec_matches);
        reveal(ContextNodeId::spec_bytes);
    }

    pub(crate) proof fn order_reflexive(value: &Self)
        ensures value.spec_order(value) == Ordering::Equal,
    {
        order_reflexive_from(value.spec_bytes()@, 0);
    }

    pub(crate) proof fn order_transitive(left: &Self, middle: &Self, right: &Self)
        requires
            left.spec_order(middle) == Ordering::Less,
            middle.spec_order(right) == Ordering::Less,
        ensures left.spec_order(right) == Ordering::Less,
    {
        order_transitive_from(
            left.spec_bytes()@,
            middle.spec_bytes()@,
            right.spec_bytes()@,
            0,
        );
    }
}

/// Content digest identifying one compaction-policy revision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompactionPolicyId(Sha256Digest);

impl CompactionPolicyId {
    /// Logical view of the exact policy digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }

    /// Wraps the caller-computed policy digest without adding authenticity semantics.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (result: Self)
        ensures result.spec_digest() == digest,
    { Self(digest) }

    /// Returns the exact policy digest.
    #[must_use]
    pub const fn digest(self) -> (digest: Sha256Digest)
        ensures digest == self.spec_digest(),
    { self.0 }
}

/// Content digest identifying one immutable context plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContextPlanId(Sha256Digest);

impl ContextPlanId {
    /// Logical view of the exact plan digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }

    /// Wraps the caller-computed plan digest without adding authenticity semantics.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (result: Self)
        ensures result.spec_digest() == digest,
    { Self(digest) }

    /// Returns the exact plan digest.
    #[must_use]
    pub const fn digest(self) -> (digest: Sha256Digest)
        ensures digest == self.spec_digest(),
    { self.0 }
}

fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> (equal: bool)
    ensures equal == (*left == *right),
{
    let mut index = 0;
    while index < N
        invariant
            index <= N,
            forall |prior: int| 0 <= prior < index ==> left[prior] == right[prior],
        decreases N - index,
    {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    assert(*left =~= *right);
    true
}

fn compare_bytes_from<const N: usize>(
    left: &[u8; N],
    right: &[u8; N],
    index: usize,
) -> (order: Ordering)
    requires index <= N,
    ensures order == peritus_types::canonical_byte_order_from(left@, right@, index as nat),
    decreases N - index,
{
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

proof fn order_reflexive_from(bytes: Seq<u8>, index: nat)
    requires index <= bytes.len(),
    ensures peritus_types::canonical_byte_order_from(bytes, bytes, index) == Ordering::Equal,
    decreases bytes.len() - index,
{
    if index < bytes.len() {
        order_reflexive_from(bytes, index + 1);
    }
}

proof fn order_transitive_from(
    left: Seq<u8>,
    middle: Seq<u8>,
    right: Seq<u8>,
    index: nat,
)
    requires
        left.len() == middle.len(),
        middle.len() == right.len(),
        index <= left.len(),
        peritus_types::canonical_byte_order_from(left, middle, index) == Ordering::Less,
        peritus_types::canonical_byte_order_from(middle, right, index) == Ordering::Less,
    ensures peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Less,
    decreases left.len() - index,
{
    if index < left.len()
        && left[index as int] == middle[index as int]
        && middle[index as int] == right[index as int]
    {
        order_transitive_from(left, middle, right, index + 1);
    }
}

} // verus!
