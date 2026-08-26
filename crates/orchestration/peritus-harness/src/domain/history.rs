//! Bounded append-only branched revision history and rollback ancestry.

use crate::domain::graph_validation::{decode_limits, encode_limits, u64_len};
use crate::domain::{
    CanonicalEncoder, CanonicalReader, HarnessDomainError, HarnessDomainErrorKind,
    HarnessLimitKind, HarnessLimits, HarnessRevision, HarnessRevisionIdentity, RevisionDigest,
};

const HISTORY_DOMAIN: &[u8] = b"peritus-e1-harness-history-v1\0";

/// Checked source and strict-ancestor target of a rollback materialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RollbackSelection {
    source: HarnessRevisionIdentity,
    target: HarnessRevisionIdentity,
}

impl RollbackSelection {
    /// Returns the selected source revision.
    #[must_use]
    pub const fn source(self) -> HarnessRevisionIdentity {
        self.source
    }
    /// Returns the existing ancestor to materialize.
    #[must_use]
    pub const fn target(self) -> HarnessRevisionIdentity {
        self.target
    }
}

/// One bounded append-only DAG keyed by full revision digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessHistory {
    revisions: Vec<HarnessRevision>,
    limits: HarnessLimits,
}

impl HarnessHistory {
    /// Starts history with exactly one valid genesis revision.
    ///
    /// # Errors
    ///
    /// Rejects a non-genesis value or a state exceeding configured history/state bounds.
    pub fn new(
        genesis: HarnessRevision,
        limits: HarnessLimits,
    ) -> Result<Self, HarnessDomainError> {
        if genesis.predecessor().is_some()
            || genesis.number() != peritus_types::RevisionNumber::first()
        {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::GenesisConflict));
        }
        let history = Self { revisions: vec![genesis], limits };
        history.validate_encoded_bound()?;
        Ok(history)
    }

    /// Appends one exact direct successor; branches are retained.
    ///
    /// # Errors
    ///
    /// Rejects duplicates, foreign lineages, orphans, non-direct successors, and bounds.
    pub fn append(&mut self, revision: HarnessRevision) -> Result<(), HarnessDomainError> {
        if let Some(existing) = self.revision(revision.digest()) {
            let kind = if existing.canonical_bytes() == revision.canonical_bytes() {
                HarnessDomainErrorKind::DuplicateRevision
            } else {
                HarnessDomainErrorKind::CanonicalDigestMismatch
            };
            return Err(HarnessDomainError::plain(kind));
        }
        if revision.harness_id() != self.revisions[0].harness_id() {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::HarnessIdentityMismatch));
        }
        let predecessor_digest = revision
            .predecessor()
            .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::GenesisConflict))?;
        let predecessor = self
            .revision(predecessor_digest)
            .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::OrphanRevision))?;
        if !revision.is_direct_successor_of(predecessor) {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::PredecessorMismatch));
        }
        let next_count = u64_len(self.revisions.len())
            .checked_add(1)
            .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::ArithmeticOverflow))?;
        if next_count > self.limits.max_revision_history() {
            return Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::HistoryLimitExceeded,
                HarnessLimitKind::RevisionHistory,
                self.limits.max_revision_history(),
                next_count,
            ));
        }
        self.revisions.push(revision);
        if let Err(error) = self.validate_encoded_bound() {
            let _removed = self.revisions.pop();
            return Err(error);
        }
        Ok(())
    }

    /// Reconstructs history from a deterministic canonical snapshot.
    ///
    /// # Errors
    ///
    /// Rejects malformed bytes, widened limits, or any invalid append sequence.
    pub fn decode_canonical_snapshot(
        bytes: &[u8],
        ceiling: HarnessLimits,
    ) -> Result<Self, HarnessDomainError> {
        if u64_len(bytes.len()) > ceiling.max_state_bytes() {
            return Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::TotalBytesExceeded,
                HarnessLimitKind::StateBytes,
                ceiling.max_state_bytes(),
                u64_len(bytes.len()),
            ));
        }
        let mut reader = CanonicalReader::new(bytes, HISTORY_DOMAIN)?;
        let limits = decode_limits(&mut reader)?;
        if !limits.is_within(ceiling) {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::LimitWidening));
        }
        let count = reader.length()?;
        if count == 0 || u64_len(count) > limits.max_revision_history() {
            return Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::HistoryLimitExceeded,
                HarnessLimitKind::RevisionHistory,
                limits.max_revision_history(),
                u64_len(count),
            ));
        }
        let mut encoded_revisions = Vec::with_capacity(count);
        for _ in 0..count {
            encoded_revisions.push(reader.byte_slice()?.to_vec());
        }
        reader.finish()?;
        let genesis = HarnessRevision::decode_canonical(&encoded_revisions[0], None)?;
        let mut history = Self::new(genesis, limits)?;
        for encoded in &encoded_revisions[1..] {
            let predecessor_digest = HarnessRevision::predecessor_from_canonical(encoded)?
                .ok_or_else(|| {
                    HarnessDomainError::plain(HarnessDomainErrorKind::GenesisConflict)
                })?;
            let predecessor = history
                .revision(predecessor_digest)
                .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::OrphanRevision))?;
            let revision = HarnessRevision::decode_canonical(encoded, Some(predecessor))?;
            history.append(revision)?;
        }
        if history.canonical_snapshot() != bytes {
            return Err(HarnessDomainError::plain(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
            ));
        }
        Ok(history)
    }

    /// Looks up one full revision digest.
    #[must_use]
    pub fn revision(&self, digest: RevisionDigest) -> Option<&HarnessRevision> {
        self.revisions.iter().find(|revision| revision.digest() == digest)
    }

    /// Borrows revisions in append order.
    #[must_use]
    pub fn revisions(&self) -> &[HarnessRevision] {
        &self.revisions
    }

    /// Returns the unique genesis revision.
    #[must_use]
    pub fn genesis(&self) -> &HarnessRevision {
        &self.revisions[0]
    }

    /// Returns the retained revision count.
    #[must_use]
    pub const fn revision_count(&self) -> usize {
        self.revisions.len()
    }

    /// Returns the configured history limits.
    #[must_use]
    pub const fn limits(&self) -> HarnessLimits {
        self.limits
    }

    /// Returns whether `candidate` is a strict ancestor of `source`.
    #[must_use]
    pub fn is_ancestor(&self, candidate: RevisionDigest, source: RevisionDigest) -> bool {
        if candidate == source || self.revision(candidate).is_none() {
            return false;
        }
        let mut cursor = self.revision(source).and_then(HarnessRevision::predecessor);
        let mut traversed = 0_usize;
        while let Some(digest) = cursor {
            if digest == candidate {
                return true;
            }
            traversed += 1;
            if traversed > self.revisions.len() {
                return false;
            }
            cursor = self.revision(digest).and_then(HarnessRevision::predecessor);
        }
        false
    }

    /// Returns strict ancestors from direct parent through genesis.
    ///
    /// # Errors
    ///
    /// Rejects an absent source or a corrupt retained predecessor chain.
    pub fn ancestors(
        &self,
        source: RevisionDigest,
    ) -> Result<Vec<&HarnessRevision>, HarnessDomainError> {
        let source = self.revision(source).ok_or_else(|| {
            HarnessDomainError::plain(HarnessDomainErrorKind::RollbackSourceMissing)
        })?;
        let mut ancestors = Vec::new();
        let mut cursor = source.predecessor();
        while let Some(digest) = cursor {
            let revision = self
                .revision(digest)
                .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::OrphanRevision))?;
            ancestors.push(revision);
            if ancestors.len() > self.revisions.len() {
                return Err(HarnessDomainError::plain(HarnessDomainErrorKind::DependencyCycle));
            }
            cursor = revision.predecessor();
        }
        Ok(ancestors)
    }

    /// Returns direct children of a revision in deterministic full-digest order.
    #[must_use]
    pub fn children(&self, parent: RevisionDigest) -> Vec<&HarnessRevision> {
        let mut children: Vec<_> = self
            .revisions
            .iter()
            .filter(|revision| revision.predecessor() == Some(parent))
            .collect();
        children.sort_by_key(|revision| revision.digest());
        children
    }

    /// Returns revisions with no children in deterministic full-digest order.
    #[must_use]
    pub fn branch_tips(&self) -> Vec<&HarnessRevision> {
        let mut tips: Vec<_> = self
            .revisions
            .iter()
            .filter(|candidate| {
                !self
                    .revisions
                    .iter()
                    .any(|revision| revision.predecessor() == Some(candidate.digest()))
            })
            .collect();
        tips.sort_by_key(|revision| revision.digest());
        tips
    }

    /// Validates a strict ancestor-only rollback selection without mutating history.
    ///
    /// # Errors
    ///
    /// Rejects an absent source or target and any target that is not a strict source ancestor.
    pub fn validate_rollback(
        &self,
        source: RevisionDigest,
        target: RevisionDigest,
    ) -> Result<RollbackSelection, HarnessDomainError> {
        let source_revision = self.revision(source).ok_or_else(|| {
            HarnessDomainError::plain(HarnessDomainErrorKind::RollbackSourceMissing)
        })?;
        let target_revision = self.revision(target).ok_or_else(|| {
            HarnessDomainError::plain(HarnessDomainErrorKind::RollbackTargetMissing)
        })?;
        if !self.is_ancestor(target, source) {
            return Err(HarnessDomainError::plain(HarnessDomainErrorKind::RollbackNotAncestor));
        }
        Ok(RollbackSelection {
            source: source_revision.identity(),
            target: target_revision.identity(),
        })
    }

    /// Returns a deterministic parent-before-child canonical snapshot.
    #[must_use]
    pub fn canonical_snapshot(&self) -> Vec<u8> {
        let mut revisions: Vec<_> = self.revisions.iter().collect();
        revisions.sort_by_key(|revision| (revision.number(), revision.digest()));
        let mut encoder = CanonicalEncoder::new(HISTORY_DOMAIN);
        encode_limits(&mut encoder, self.limits);
        encoder.len(revisions.len());
        for revision in revisions {
            encoder.bytes(&revision.canonical_bytes());
        }
        encoder.into_bytes()
    }

    fn validate_encoded_bound(&self) -> Result<(), HarnessDomainError> {
        let actual = u64_len(self.canonical_snapshot().len());
        if actual > self.limits.max_state_bytes() {
            Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::TotalBytesExceeded,
                HarnessLimitKind::StateBytes,
                self.limits.max_state_bytes(),
                actual,
            ))
        } else {
            Ok(())
        }
    }
}
