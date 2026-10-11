//! Explicit single-tier policy-amendment preview without activation authority.

use crate::{PolicyDefinition, PolicyTier, RestrictionLayer};
use peritus_types::{PolicyId, RevisionTuple, Sha256Digest};

#[doc = " Checked request to replace or add one exact restriction tier in a successor policy."]
#[derive(Debug, Eq, PartialEq)]
pub struct PolicyAmendmentProposal {
    base_policy_id: PolicyId,
    successor_policy_id: PolicyId,
    tier: PolicyTier,
    replacement: RestrictionLayer,
    amendment_digest: Sha256Digest,
}
mod preview;
mod proposal;
#[doc = " Complete checked successor preview. It is not an active-policy fact or effect permit."]
#[derive(Debug, Eq, PartialEq)]
pub struct PolicyRevisionCandidate {
    base_policy_id: PolicyId,
    successor_policy: PolicyDefinition,
    tier: PolicyTier,
    amendment_digest: Sha256Digest,
}
impl PolicyRevisionCandidate {
    #[doc = " Returns the exact immutable base policy identity."]
    #[must_use]
    pub const fn base_policy_id(&self) -> PolicyId {
        self.base_policy_id
    }
    #[doc = " Returns the fresh proposed successor policy identity."]
    #[must_use]
    pub const fn successor_policy_id(&self) -> PolicyId {
        self.successor_policy.policy_id()
    }
    #[doc = " Returns the sole ordinary tier replaced by the candidate."]
    #[must_use]
    pub const fn tier(&self) -> PolicyTier {
        self.tier
    }
    #[doc = " Returns the exact amendment digest."]
    #[must_use]
    pub const fn amendment_digest(&self) -> Sha256Digest {
        self.amendment_digest
    }
    #[doc = " Borrows the complete checked successor policy preview."]
    #[must_use]
    pub const fn successor_policy(&self) -> &PolicyDefinition {
        &self.successor_policy
    }
}
const fn successor_revision(base: RevisionTuple, successor_policy_id: PolicyId) -> RevisionTuple {
    RevisionTuple::new(
        base.acceptance_spec_id(),
        base.harness_id(),
        base.workspace_id(),
        base.workspace_generation(),
        base.workspace_revision(),
        successor_policy_id,
        base.provider_profile_id(),
    )
}
fn amended_layers_from(
    current: &[RestrictionLayer],
    target: PolicyTier,
    replacement: &RestrictionLayer,
    revision: RevisionTuple,
    index: usize,
    inserted: bool,
) -> Vec<RestrictionLayer> {
    if index >= current.len() {
        let mut layers = Vec::new();
        if !inserted {
            layers.push(replacement.rebind_revision(revision));
        }
        return layers;
    }
    if !inserted && current[index].tier().rank() >= target.rank() {
        let mut layers = amended_layers_from(current, target, replacement, revision, index, true);
        let rebound = replacement.rebind_revision(revision);
        layers.insert(0, rebound);
        return layers;
    }
    if current[index].tier().rank() == target.rank() {
        if !inserted {}
        let layers =
            amended_layers_from(current, target, replacement, revision, index + 1, inserted);
        return layers;
    }
    if !inserted {}
    let mut layers =
        amended_layers_from(current, target, replacement, revision, index + 1, inserted);
    let rebound = current[index].rebind_revision(revision);
    layers.insert(0, rebound);
    layers
}
