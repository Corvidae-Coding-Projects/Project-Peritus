//! Explicit single-tier policy-amendment preview without activation authority.

use crate::{PolicyDefinition, PolicyTier, RestrictionLayer};
use peritus_types::{PolicyId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Checked request to replace or add one exact restriction tier in a successor policy.
#[derive(Debug, Eq, PartialEq)]
pub struct PolicyAmendmentProposal {
    base_policy_id: PolicyId,
    successor_policy_id: PolicyId,
    tier: PolicyTier,
    replacement: RestrictionLayer,
    amendment_digest: Sha256Digest,
}

mod proposal;
mod preview;

/// Complete checked successor preview. It is not an active-policy fact or effect permit.
#[derive(Debug, Eq, PartialEq)]
pub struct PolicyRevisionCandidate {
    base_policy_id: PolicyId,
    successor_policy: PolicyDefinition,
    tier: PolicyTier,
    amendment_digest: Sha256Digest,
}

impl PolicyRevisionCandidate {
    /// Returns the exact immutable base policy identity value.
    pub closed spec fn spec_base_policy_id_value(&self) -> PolicyId { self.base_policy_id }

    /// Returns the exact fresh successor policy identity value.
    pub closed spec fn spec_successor_policy_id_value(&self) -> PolicyId {
        self.successor_policy.spec_policy_id_value()
    }

    /// Returns the exact amendment digest value.
    pub closed spec fn spec_amendment_digest_value(&self) -> Sha256Digest {
        self.amendment_digest
    }

    /// Returns whether this candidate is the exact restriction-only successor of a base policy.
    pub closed spec fn spec_is_exact_amendment_of(
        &self,
        base: &PolicyDefinition,
        proposal: &PolicyAmendmentProposal,
    ) -> bool {
        let revision = self.successor_policy.spec_boundary_revision();
        self.spec_base_policy_id() == proposal.spec_base_policy_id()
            && self.spec_successor_policy_id() == proposal.spec_successor_policy_id()
            && self.spec_tier() == proposal.spec_tier()
            && self.spec_amendment_digest() == proposal.spec_amendment_digest()
            && crate::amendment_model::revision_is_exact_successor(
                base.spec_boundary_revision(),
                revision,
                proposal.spec_successor_policy_id(),
            )
            && self.successor_policy.spec_ceiling_is_revision_rebind_of(base, revision)
            && self.successor_policy.spec_operations_same_as(base)
            && crate::amendment_model::exact_amended_layers_from(
                base.spec_layers(),
                self.successor_policy.spec_layers(),
                proposal.spec_tier(),
                &proposal.spec_replacement(),
                revision,
                0,
                false,
            )
    }
    /// Returns the exact immutable base policy identity bytes used by specifications.
    pub closed spec fn spec_base_policy_id(&self) -> [u8; 16] {
        self.base_policy_id.spec_bytes()
    }

    /// Returns the exact fresh successor policy identity bytes used by specifications.
    pub closed spec fn spec_successor_policy_id(&self) -> [u8; 16] {
        self.successor_policy.spec_policy_id()
    }

    /// Returns the sole replaced tier used by specifications.
    pub closed spec fn spec_tier(&self) -> PolicyTier { self.tier }

    /// Returns the exact amendment digest bytes used by specifications.
    pub closed spec fn spec_amendment_digest(&self) -> [u8; 32] {
        self.amendment_digest.spec_bytes()
    }

    /// Returns the exact immutable base policy identity.
    #[must_use]
    pub const fn base_policy_id(&self) -> (policy_id: PolicyId)
        ensures policy_id == self.spec_base_policy_id_value(),
    { self.base_policy_id }

    /// Returns the fresh proposed successor policy identity.
    #[must_use]
    pub const fn successor_policy_id(&self) -> (policy_id: PolicyId)
        ensures
            policy_id == self.spec_successor_policy_id_value(),
            policy_id.spec_bytes() == self.spec_successor_policy_id(),
    {
        self.successor_policy.policy_id()
    }

    /// Returns the sole ordinary tier replaced by the candidate.
    #[must_use]
    pub const fn tier(&self) -> (tier: PolicyTier)
        ensures tier == self.spec_tier(),
    { self.tier }

    /// Returns the exact amendment digest.
    #[must_use]
    pub const fn amendment_digest(&self) -> (digest: Sha256Digest)
        ensures
            digest == self.spec_amendment_digest_value(),
            digest.spec_bytes() == self.spec_amendment_digest(),
    { self.amendment_digest }

    /// Borrows the complete checked successor policy preview.
    #[must_use]
    pub const fn successor_policy(&self) -> &PolicyDefinition { &self.successor_policy }
}

const fn successor_revision(
    base: RevisionTuple,
    successor_policy_id: PolicyId,
) -> (revision: RevisionTuple)
    ensures
        crate::amendment_model::revision_is_exact_successor(
            base,
            revision,
            successor_policy_id.spec_bytes(),
        ),
{
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
) -> (layers: Vec<RestrictionLayer>)
    ensures
        crate::amendment_model::exact_amended_layers_from(
            current@,
            layers@,
            target,
            replacement,
            revision,
            index as nat,
            inserted,
        ),
    decreases
        current.len() - index,
        if inserted { 0usize } else { 1usize },
{
    if index >= current.len() {
        let mut layers = Vec::new();
        if !inserted {
            layers.push(replacement.rebind_revision(revision));
        }
        return layers;
    }
    if !inserted && current[index].tier().rank() >= target.rank() {
        assert(current@[index as int].spec_tier().spec_rank() >= target.spec_rank());
        let mut layers = amended_layers_from(
            current,
            target,
            replacement,
            revision,
            index,
            true,
        );
        let ghost tail = layers@;
        let rebound = replacement.rebind_revision(revision);
        layers.insert(0, rebound);
        assert(layers@[0].spec_is_revision_rebind_of(replacement, revision));
        assert(layers@.subrange(1, layers@.len() as int) == tail);
        assert(crate::amendment_model::exact_amended_layers_from(
            current@,
            layers@,
            target,
            replacement,
            revision,
            index as nat,
            inserted,
        ));
        return layers;
    }
    if current[index].tier().rank() == target.rank() {
        assert(current@[index as int].spec_tier().spec_rank() == target.spec_rank());
        if !inserted {
            assert(current@[index as int].spec_tier().spec_rank() < target.spec_rank());
        }
        let layers = amended_layers_from(
            current,
            target,
            replacement,
            revision,
            index + 1,
            inserted,
        );
        assert(crate::amendment_model::exact_amended_layers_from(
            current@,
            layers@,
            target,
            replacement,
            revision,
            index as nat,
            inserted,
        ));
        return layers;
    }
    if !inserted {
        assert(current@[index as int].spec_tier().spec_rank() < target.spec_rank());
    }
    assert(current@[index as int].spec_tier().spec_rank() != target.spec_rank());
    let mut layers = amended_layers_from(
        current,
        target,
        replacement,
        revision,
        index + 1,
        inserted,
    );
    let ghost tail = layers@;
    let rebound = current[index].rebind_revision(revision);
    layers.insert(0, rebound);
    assert(layers@[0].spec_is_revision_rebind_of(&current@[index as int], revision));
    assert(layers@.subrange(1, layers@.len() as int) == tail);
    assert(crate::amendment_model::exact_amended_layers_from(
        current@,
        layers@,
        target,
        replacement,
        revision,
        index as nat,
        inserted,
    ));
    layers
}

} // verus!
