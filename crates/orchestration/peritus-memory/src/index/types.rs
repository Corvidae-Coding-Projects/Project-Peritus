//! Immutable canonical index views and posting lists.

use crate::{
    ClaimType, FeatureKey, MemoryError, MemoryId, MemoryRecord, MemoryScope, MemoryTombstone,
    RetrievalPlan, RetrievalPolicy, RetrievalQuery,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Canonical posting list for one exact scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopePosting {
    scope: MemoryScope,
    memory_ids: Vec<MemoryId>,
}

impl ScopePosting {
    pub(crate) const fn new(scope: MemoryScope, memory_ids: Vec<MemoryId>) -> Self {
        Self { scope, memory_ids }
    }

    pub(crate) fn push(&mut self, id: MemoryId) { self.memory_ids.push(id); }

    /// Returns the exact posting scope.
    #[must_use]
    pub const fn scope(&self) -> &MemoryScope { &self.scope }

    /// Returns active identifiers in canonical order.
    #[must_use]
    pub const fn memory_ids(&self) -> &[MemoryId] { self.memory_ids.as_slice() }
}

/// Canonical posting list for one typed claim category.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimPosting {
    claim_type: ClaimType,
    memory_ids: Vec<MemoryId>,
}

impl ClaimPosting {
    pub(crate) const fn new(claim_type: ClaimType, memory_ids: Vec<MemoryId>) -> Self {
        Self { claim_type, memory_ids }
    }

    pub(crate) fn push(&mut self, id: MemoryId) { self.memory_ids.push(id); }

    /// Returns the posting claim category.
    #[must_use]
    pub const fn claim_type(&self) -> ClaimType { self.claim_type }

    /// Returns active identifiers in canonical order.
    #[must_use]
    pub const fn memory_ids(&self) -> &[MemoryId] { self.memory_ids.as_slice() }
}

/// Canonical posting list for one retrieval feature key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeaturePosting {
    key: FeatureKey,
    memory_ids: Vec<MemoryId>,
}

impl FeaturePosting {
    pub(crate) const fn new(key: FeatureKey, memory_ids: Vec<MemoryId>) -> Self {
        Self { key, memory_ids }
    }

    pub(crate) fn push(&mut self, id: MemoryId) { self.memory_ids.push(id); }

    /// Returns the stable feature key.
    #[must_use]
    pub const fn key(&self) -> FeatureKey { self.key }

    /// Returns active identifiers in canonical order.
    #[must_use]
    pub const fn memory_ids(&self) -> &[MemoryId] { self.memory_ids.as_slice() }
}

/// Rebuildable active memory view with canonical posting lists and SHA-256 digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryIndex {
    active_records: Vec<MemoryRecord>,
    tombstones: Vec<MemoryTombstone>,
    scopes: Vec<ScopePosting>,
    claims: Vec<ClaimPosting>,
    features: Vec<FeaturePosting>,
    digest: Sha256Digest,
}

impl MemoryIndex {
    pub(crate) const fn from_parts(
        active_records: Vec<MemoryRecord>,
        tombstones: Vec<MemoryTombstone>,
        scopes: Vec<ScopePosting>,
        claims: Vec<ClaimPosting>,
        features: Vec<FeaturePosting>,
        digest: Sha256Digest,
    ) -> Self {
        Self { active_records, tombstones, scopes, claims, features, digest }
    }

    /// Rebuilds an index from canonical `(memory ID, revision)` ordered snapshots and tombstones.
    ///
    /// Later revisions replace earlier revisions. A tombstone suppresses every record at or below
    /// its bound revision, and inactive latest records do not enter active postings.
    ///
    /// # Errors
    ///
    /// Returns a typed error for excessive, unordered, duplicate, or digest-conflicting replay.
    #[cfg(not(verus_only))]
    pub fn rebuild(
        records: Vec<MemoryRecord>,
        tombstones: Vec<MemoryTombstone>,
    ) -> Result<Self, MemoryError> {
        let mut index = super::rebuild::rebuild_unhashed(records, tombstones)?;
        index.digest = super::canonical::index_digest(
            index.active_records.as_slice(),
            index.tombstones.as_slice(),
        );
        Ok(index)
    }

    /// Returns canonical active records in stable identifier order.
    #[must_use]
    pub const fn active_records(&self) -> &[MemoryRecord] { self.active_records.as_slice() }

    /// Returns canonical retained tombstones.
    #[must_use]
    pub const fn tombstones(&self) -> &[MemoryTombstone] { self.tombstones.as_slice() }

    /// Returns exact-scope posting lists in canonical scope order.
    #[must_use]
    pub const fn scope_postings(&self) -> &[ScopePosting] { self.scopes.as_slice() }

    /// Returns claim posting lists in canonical claim order.
    #[must_use]
    pub const fn claim_postings(&self) -> &[ClaimPosting] { self.claims.as_slice() }

    /// Returns feature posting lists in canonical key order.
    #[must_use]
    pub const fn feature_postings(&self) -> &[FeaturePosting] { self.features.as_slice() }

    /// Returns SHA-256 of the versioned canonical active view and tombstones.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }

    /// Retrieves against the canonical active view. This matches a full scan of that same view.
    ///
    /// # Errors
    ///
    /// Returns the same typed planning errors as [`crate::retrieve`].
    pub fn retrieve(
        &self,
        policy: &RetrievalPolicy,
        query: &RetrievalQuery,
    ) -> Result<RetrievalPlan, MemoryError> {
        crate::retrieve(self.active_records.as_slice(), &[], policy, query)
    }
}

} // verus!
