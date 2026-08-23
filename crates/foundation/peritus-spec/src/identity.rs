//! Stable content-addressed identifiers owned by acceptance specifications.

use peritus_types::{AcceptanceSpecId, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Returns mathematical equality of the exact acceptance-identifier byte representations.
pub open spec fn acceptance_ids_match(
    left: AcceptanceSpecId,
    right: AcceptanceSpecId,
) -> bool {
    let left = left.spec_bytes();
    let right = right.spec_bytes();
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for the cross-module executable equality bridge"
)]
pub(crate) const fn acceptance_id_matches(
    left: AcceptanceSpecId,
    right: AcceptanceSpecId,
) -> (matches: bool)
    ensures matches == acceptance_ids_match(left, right),
{
    let left = left.as_bytes();
    let right = right.as_bytes();
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
}

/// References immutable content stored outside this pure domain crate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentReference(Sha256Digest);

impl ContentReference {
    /// Creates a reference from an already-computed digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (result: Self)
        ensures result.spec_digest() == digest
    { Self(digest) }

    /// Returns the specification view of the digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_digest()
    { self.0 }
}

/// Stable identifier for one immutable requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequirementId(Sha256Digest);

impl RequirementId {
    /// Creates an identifier from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (result: Self)
        ensures result.spec_digest() == digest
    { Self(digest) }

    /// Returns the specification view of the digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_digest()
    { self.0 }
}

/// Stable identifier for one required evidence declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceRequirementId(Sha256Digest);

impl EvidenceRequirementId {
    /// Creates an identifier from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (result: Self)
        ensures result.spec_digest() == digest
    { Self(digest) }

    /// Returns the specification view of the digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_digest()
    { self.0 }
}

/// Stable content-addressed review category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewCategory(Sha256Digest);

impl ReviewCategory {
    /// Creates a category from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (result: Self)
        ensures result.spec_digest() == digest
    { Self(digest) }

    /// Returns the specification view of the digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_digest()
    { self.0 }
}

} // verus!
