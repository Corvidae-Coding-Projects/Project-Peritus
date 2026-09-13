//! Stable caller-supplied knowledge identities.

mod collection;
pub use collection::validate_section_ids;

use crate::{KnowledgeError, KnowledgeErrorKind};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Stable 128-bit identity of one logical knowledge section.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeSectionId([u8; 16]);

impl KnowledgeSectionId {
    /// Logical view of every identity byte.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0 }

    /// Exact byte identity equality.
    pub open spec fn spec_matches(&self, other: &Self) -> bool {
        self.spec_bytes() == other.spec_bytes()
    }

    /// Exact canonical lexicographic order of every identity byte.
    pub open spec fn spec_order(&self, other: &Self) -> Ordering {
        peritus_types::canonical_byte_order_from(
            self.spec_bytes()@,
            other.spec_bytes()@,
            0,
        )
    }

    /// Creates a nonzero section identity.
    ///
    /// # Errors
    ///
    /// Returns [`KnowledgeErrorKind::ZeroIdentifier`] for the all-zero representation.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, KnowledgeError>)
        ensures
            result.is_ok() == (exists |index: int| 0 <= index < 16 && bytes[index] != 0),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_plain(KnowledgeErrorKind::ZeroIdentifier),
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
        Err(KnowledgeError::plain(KnowledgeErrorKind::ZeroIdentifier))
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes(),
    { &self.0 }

    /// Returns whether every identity byte is equal.
    #[must_use]
    pub fn matches(&self, other: &Self) -> (matches: bool)
        ensures matches == self.spec_matches(other),
    {
        bytes_equal(self.as_bytes(), other.as_bytes())
    }

    /// Reverses exact section-identity equality inside proofs.
    pub(crate) proof fn matches_symmetric(left: &Self, right: &Self)
        requires left.spec_matches(right),
        ensures right.spec_matches(left),
    {
        assert(left.spec_bytes() == right.spec_bytes());
    }

    /// Establishes exact section-identity equality with itself inside proofs.
    pub(crate) proof fn matches_reflexive(value: &Self)
        ensures value.spec_matches(value),
    {
    }

    /// Converts exact byte matching to structural section-identity equality.
    pub(crate) proof fn matches_implies_equal(left: &Self, right: &Self)
        requires left.spec_matches(right),
        ensures *left == *right,
    {
        assert(left.spec_bytes() == right.spec_bytes());
        assert(left.0 == right.0);
    }

    /// Composes exact section-identity equality inside proofs.
    pub(crate) proof fn matches_transitive(left: &Self, middle: &Self, right: &Self)
        requires left.spec_matches(middle), middle.spec_matches(right),
        ensures left.spec_matches(right),
    {
        assert(left.spec_bytes() == middle.spec_bytes());
        assert(middle.spec_bytes() == right.spec_bytes());
    }

    pub(crate) fn canonical_order(&self, other: &Self) -> (order: Ordering)
        ensures order == self.spec_order(other),
    {
        compare_bytes_from(self.as_bytes(), other.as_bytes(), 0)
    }
}

/// Stable 128-bit identity of one authoritative source path or input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeSourceId([u8; 16]);

impl KnowledgeSourceId {
    /// Logical view of every identity byte.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0 }

    /// Exact byte identity equality.
    pub open spec fn spec_matches(&self, other: &Self) -> bool {
        self.spec_bytes() == other.spec_bytes()
    }

    /// Exact canonical lexicographic order of every identity byte.
    pub open spec fn spec_order(&self, other: &Self) -> Ordering {
        peritus_types::canonical_byte_order_from(self.spec_bytes()@, other.spec_bytes()@, 0)
    }

    pub(crate) fn canonical_order(&self, other: &Self) -> (order: Ordering)
        ensures order == self.spec_order(other),
    {
        compare_bytes_from(self.as_bytes(), other.as_bytes(), 0)
    }

    /// Creates a nonzero source identity.
    ///
    /// # Errors
    ///
    /// Returns [`KnowledgeErrorKind::ZeroIdentifier`] for the all-zero representation.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, KnowledgeError>)
        ensures
            result.is_ok() == (exists |index: int| 0 <= index < 16 && bytes[index] != 0),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_plain(KnowledgeErrorKind::ZeroIdentifier),
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
        Err(KnowledgeError::plain(KnowledgeErrorKind::ZeroIdentifier))
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes(),
    { &self.0 }

    /// Returns whether every identity byte is equal.
    #[must_use]
    pub fn matches(&self, other: &Self) -> (matches: bool)
        ensures matches == self.spec_matches(other),
    {
        bytes_equal(self.as_bytes(), other.as_bytes())
    }
}

/// Returns whether two fixed-size byte arrays are identical.
pub fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> (equal: bool)
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

} // verus!
