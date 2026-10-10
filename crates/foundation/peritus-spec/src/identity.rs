//! Stable content-addressed identifiers owned by acceptance specifications.

use peritus_types::{AcceptanceSpecId, Sha256Digest};

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for the cross-module executable equality bridge"
)]
pub(crate) const fn acceptance_id_matches(left: AcceptanceSpecId, right: AcceptanceSpecId) -> bool {
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
#[doc = " References immutable content stored outside this pure domain crate."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentReference(Sha256Digest);
impl ContentReference {
    #[doc = " Creates a reference from an already-computed digest."]
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }
    #[doc = " Returns the exact digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.0
    }
}
#[doc = " Stable identifier for one immutable requirement."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequirementId(Sha256Digest);
impl RequirementId {
    #[doc = " Creates an identifier from its canonical digest."]
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }
    #[doc = " Returns the exact digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.0
    }
}
#[doc = " Stable identifier for one required evidence declaration."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceRequirementId(Sha256Digest);
impl EvidenceRequirementId {
    #[doc = " Creates an identifier from its canonical digest."]
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }
    #[doc = " Returns the exact digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.0
    }
}
#[doc = " Stable content-addressed review category."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewCategory(Sha256Digest);
impl ReviewCategory {
    #[doc = " Creates a category from its canonical digest."]
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }
    #[doc = " Returns the exact digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.0
    }
}
