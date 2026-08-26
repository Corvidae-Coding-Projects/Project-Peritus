//! Rebuildable read-only harness query projection.

use std::collections::{BTreeMap, BTreeSet};

use peritus_types::{HarnessId, Sha256Digest};

use crate::{
    aggregate::{AggregateError, HarnessEvent, HarnessState, PendingMaterialization},
    domain::{
        ComponentDeclaration, HarnessRevision, ProtectedAsset, RevisionDigest, RollbackSelection,
    },
    materialization::{
        MaterializationFailure, MaterializationPlanId, MaterializationReceipt,
        MaterializationReceiptId,
    },
};

/// Complete read-only projection reconstructed solely from immutable semantic events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessProjection {
    state: HarnessState,
}

impl HarnessProjection {
    /// Rebuilds a projection from genesis through an exact contiguous event sequence.
    ///
    /// # Errors
    /// Rejects every illegal event or terminal divergence rejected by aggregate replay.
    pub fn rebuild(events: &[HarnessEvent]) -> Result<Self, AggregateError> {
        crate::replay::replay(events).map(|state| Self { state })
    }

    /// Replaces corrupted derived state by replaying immutable events again.
    ///
    /// # Errors
    /// Leaves this projection unchanged when event replay fails.
    pub fn repair(&mut self, events: &[HarnessEvent]) -> Result<(), AggregateError> {
        let rebuilt = Self::rebuild(events)?;
        *self = rebuilt;
        Ok(())
    }

    /// Returns the stable harness lineage.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.state.harness_id()
    }
    /// Returns the projected event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.state.sequence()
    }
    /// Returns revisions in append order.
    #[must_use]
    pub fn revisions(&self) -> &[HarnessRevision] {
        self.state.history().revisions()
    }
    /// Returns branch tips in deterministic digest order.
    #[must_use]
    pub fn branch_tips(&self) -> Vec<&HarnessRevision> {
        self.state.history().branch_tips()
    }
    /// Looks up one immutable revision.
    #[must_use]
    pub fn revision(&self, digest: RevisionDigest) -> Option<&HarnessRevision> {
        self.state.history().revision(digest)
    }
    /// Returns canonical component declarations for one revision.
    #[must_use]
    pub fn components(&self, digest: RevisionDigest) -> Option<&[ComponentDeclaration]> {
        self.revision(digest).map(|revision| revision.graph().declarations())
    }
    /// Returns the checked protected inventory for one revision.
    #[must_use]
    pub fn protected_assets(&self, digest: RevisionDigest) -> Option<&[ProtectedAsset]> {
        self.revision(digest).map(|revision| revision.graph().protected_assets())
    }
    /// Returns whether one revision is a strict ancestor of another.
    #[must_use]
    pub fn is_ancestor(&self, candidate: RevisionDigest, source: RevisionDigest) -> bool {
        self.state.history().is_ancestor(candidate, source)
    }
    /// Validates an ancestry-only rollback selection without changing projection state.
    ///
    /// # Errors
    /// Rejects missing or non-ancestor revisions.
    pub fn validate_rollback(
        &self,
        source: RevisionDigest,
        target: RevisionDigest,
    ) -> Result<RollbackSelection, crate::domain::HarnessDomainError> {
        self.state.history().validate_rollback(source, target)
    }
    /// Returns pending plans and delivery state in plan-identity order.
    #[must_use]
    pub const fn pending(&self) -> &BTreeMap<MaterializationPlanId, PendingMaterialization> {
        self.state.pending()
    }
    /// Returns retained exact receipts in receipt-identity order.
    #[must_use]
    pub const fn receipts(&self) -> &BTreeMap<MaterializationReceiptId, MaterializationReceipt> {
        self.state.receipts()
    }
    /// Returns retained typed failure diagnostics.
    #[must_use]
    pub fn failures(&self) -> &[MaterializationFailure] {
        self.state.failures()
    }
    /// Returns all current immutable history and hot-state artifact roots.
    #[must_use]
    pub fn artifact_roots(&self) -> BTreeSet<Sha256Digest> {
        self.state.artifact_roots()
    }
    /// Returns the exact digest of all authoritative projected state.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state.state_digest()
    }
}
