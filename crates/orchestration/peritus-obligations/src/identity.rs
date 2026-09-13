//! Stable typed identities used only by the obligation domain.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Stable identity of one public condition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConditionId(Sha256Digest);

impl ConditionId {
    /// Creates an identity from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (value: Self)
        ensures value.spec_digest() == digest,
    { Self(digest) }

    /// Returns the canonical digest.
    #[must_use]
    pub const fn digest(self) -> (value: Sha256Digest)
        ensures value == self.spec_digest(),
    { self.0 }

    /// Logical view of the complete identity digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }
}

/// Stable identity of one alternative group.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AlternativeGroupId(Sha256Digest);

impl AlternativeGroupId {
    /// Creates an identity from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (value: Self)
        ensures value.spec_digest() == digest,
    { Self(digest) }

    /// Returns the canonical digest.
    #[must_use]
    pub const fn digest(self) -> (value: Sha256Digest)
        ensures value == self.spec_digest(),
    { self.0 }

    /// Logical view of the complete identity digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }
}

/// Stable identity of one branch within an alternative group.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AlternativeBranchId(Sha256Digest);

impl AlternativeBranchId {
    /// Creates an identity from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (value: Self)
        ensures value.spec_digest() == digest,
    { Self(digest) }

    /// Returns the canonical digest.
    #[must_use]
    pub const fn digest(self) -> (value: Sha256Digest)
        ensures value == self.spec_digest(),
    { self.0 }

    /// Logical view of the complete identity digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }
}

/// Stable identity of one exact public path mention.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PathId(Sha256Digest);

impl PathId {
    /// Creates an identity from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (value: Self)
        ensures value.spec_digest() == digest,
    { Self(digest) }

    /// Returns the canonical digest.
    #[must_use]
    pub const fn digest(self) -> (value: Sha256Digest)
        ensures value == self.spec_digest(),
    { self.0 }

    /// Logical view of the complete identity digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }
}

/// Stable identity of one direction-specific schema field.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemaFieldId(Sha256Digest);

impl SchemaFieldId {
    /// Creates an identity from its canonical digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> (value: Self)
        ensures value.spec_digest() == digest,
    { Self(digest) }

    /// Returns the canonical digest.
    #[must_use]
    pub const fn digest(self) -> (value: Sha256Digest)
        ensures value == self.spec_digest(),
    { self.0 }

    /// Logical view of the complete identity digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.0 }
}

} // verus!
