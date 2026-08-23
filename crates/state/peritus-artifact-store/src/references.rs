//! Canonically ordered artifact reference sets.

use std::collections::{BTreeSet, btree_set};

use peritus_types::Sha256Digest;

use crate::ArtifactDigest;

/// Durable reference-owner family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReferenceOwnerKind {
    /// A committed journal record owns the reference.
    Journal,
    /// A durable evidence record owns the reference.
    Evidence,
}

impl ReferenceOwnerKind {
    pub(crate) const fn database_tag(self) -> i64 {
        match self {
            Self::Journal => 1,
            Self::Evidence => 2,
        }
    }
}

/// Stable owner of one durable artifact reference.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReferenceOwner {
    kind: ReferenceOwnerKind,
    identity: Sha256Digest,
}

impl ReferenceOwner {
    /// Creates a journal-owned reference identity.
    #[must_use]
    pub const fn journal(identity: Sha256Digest) -> Self {
        Self { kind: ReferenceOwnerKind::Journal, identity }
    }

    /// Creates an evidence-owned reference identity.
    #[must_use]
    pub const fn evidence(identity: Sha256Digest) -> Self {
        Self { kind: ReferenceOwnerKind::Evidence, identity }
    }

    /// Returns the owner family.
    #[must_use]
    pub const fn kind(self) -> ReferenceOwnerKind {
        self.kind
    }

    /// Returns the exact owner identity digest.
    #[must_use]
    pub const fn identity(self) -> Sha256Digest {
        self.identity
    }
}

/// A deduplicated, canonical set of artifact references.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArtifactReferenceSet {
    digests: BTreeSet<ArtifactDigest>,
}

impl ArtifactReferenceSet {
    /// Creates an empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self { digests: BTreeSet::new() }
    }

    /// Inserts a digest and returns whether it was newly present.
    pub fn insert(&mut self, digest: ArtifactDigest) -> bool {
        self.digests.insert(digest)
    }

    /// Returns whether a digest is present.
    #[must_use]
    pub fn contains(&self, digest: &ArtifactDigest) -> bool {
        self.digests.contains(digest)
    }

    /// Returns the number of unique references.
    #[must_use]
    pub fn len(&self) -> usize {
        self.digests.len()
    }

    /// Returns whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.digests.is_empty()
    }

    /// Iterates in canonical digest-byte order.
    pub fn iter(&self) -> btree_set::Iter<'_, ArtifactDigest> {
        self.into_iter()
    }
}

impl<'a> IntoIterator for &'a ArtifactReferenceSet {
    type Item = &'a ArtifactDigest;
    type IntoIter = btree_set::Iter<'a, ArtifactDigest>;

    fn into_iter(self) -> Self::IntoIter {
        self.digests.iter()
    }
}

impl FromIterator<ArtifactDigest> for ArtifactReferenceSet {
    fn from_iter<T: IntoIterator<Item = ArtifactDigest>>(iter: T) -> Self {
        Self { digests: iter.into_iter().collect() }
    }
}

/// Journal and evidence roots used by mark-and-sweep planning.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReferenceRoots {
    journal: ArtifactReferenceSet,
    evidence: ArtifactReferenceSet,
}

impl ReferenceRoots {
    /// Creates root sets from independent journal and evidence projections.
    #[must_use]
    pub const fn new(journal: ArtifactReferenceSet, evidence: ArtifactReferenceSet) -> Self {
        Self { journal, evidence }
    }

    /// Returns journal roots.
    #[must_use]
    pub const fn journal(&self) -> &ArtifactReferenceSet {
        &self.journal
    }

    /// Returns evidence roots.
    #[must_use]
    pub const fn evidence(&self) -> &ArtifactReferenceSet {
        &self.evidence
    }

    /// Returns whether either authoritative root set marks a digest.
    #[must_use]
    pub fn contains(&self, digest: &ArtifactDigest) -> bool {
        self.journal.contains(digest) || self.evidence.contains(digest)
    }

    pub(crate) fn all(&self) -> BTreeSet<ArtifactDigest> {
        self.journal.iter().chain(self.evidence.iter()).copied().collect()
    }
}
