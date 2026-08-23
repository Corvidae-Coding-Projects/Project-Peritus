//! Exact checked amendment-proposal construction and projections.

use super::PolicyAmendmentProposal;
use crate::{PolicyError, PolicyTier, RestrictionLayer};
use peritus_types::{PolicyId, Sha256Digest};
use vstd::prelude::*;

verus! {

impl PolicyAmendmentProposal {
    /// Returns the exact immutable base policy identity value.
    pub closed spec fn spec_base_policy_id_value(&self) -> PolicyId { self.base_policy_id }

    /// Returns the exact fresh successor policy identity value.
    pub closed spec fn spec_successor_policy_id_value(&self) -> PolicyId {
        self.successor_policy_id
    }

    /// Returns the exact immutable base policy identity bytes used by specifications.
    pub closed spec fn spec_base_policy_id(&self) -> [u8; 16] {
        self.base_policy_id.spec_bytes()
    }

    /// Returns the exact fresh successor policy identity bytes used by specifications.
    pub closed spec fn spec_successor_policy_id(&self) -> [u8; 16] {
        self.successor_policy_id.spec_bytes()
    }

    /// Returns the sole replacement tier used by specifications.
    pub closed spec fn spec_tier(&self) -> PolicyTier { self.tier }

    /// Returns the exact replacement layer used by amendment specifications.
    pub closed spec fn spec_replacement(&self) -> RestrictionLayer { self.replacement }

    /// Returns the exact amendment digest bytes used by specifications.
    pub closed spec fn spec_amendment_digest(&self) -> [u8; 32] {
        self.amendment_digest.spec_bytes()
    }

    /// Creates an exact single-tier amendment proposal.
    ///
    /// # Errors
    ///
    /// Returns a typed failure if the successor reuses the base identity or the replacement layer
    /// does not have the declared tier.
    pub fn new(
        base_policy_id: PolicyId,
        successor_policy_id: PolicyId,
        tier: PolicyTier,
        replacement: RestrictionLayer,
        amendment_digest: Sha256Digest,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(proposal) => {
                    !crate::model::same_identifier(
                        base_policy_id.spec_bytes(),
                        successor_policy_id.spec_bytes(),
                    )
                        && replacement.spec_tier().spec_rank() == tier.spec_rank()
                        && proposal.spec_base_policy_id() == base_policy_id.spec_bytes()
                        && proposal.spec_successor_policy_id()
                            == successor_policy_id.spec_bytes()
                        && proposal.spec_tier() == tier
                        && proposal.spec_replacement() == replacement
                        && proposal.spec_amendment_digest() == amendment_digest.spec_bytes()
                }
                Err(error) => {
                    error.spec_dimension().is_none()
                        && error.spec_collection().is_none()
                        && if crate::model::same_identifier(
                            base_policy_id.spec_bytes(),
                            successor_policy_id.spec_bytes(),
                        ) {
                            error.spec_kind() == crate::PolicyErrorKind::AmendmentPolicyIdReuse
                        } else {
                            replacement.spec_tier().spec_rank() != tier.spec_rank()
                                && error.spec_kind()
                                    == crate::PolicyErrorKind::AmendmentTierMismatch
                        }
                }
            },
    {
        if crate::identity::identifier_values_equal(
            *base_policy_id.as_bytes(),
            *successor_policy_id.as_bytes(),
        ) {
            return Err(PolicyError::amendment_policy_id_reuse());
        }
        let replacement_tier = replacement.tier();
        let replacement_rank = replacement_tier.rank();
        let tier_rank = tier.rank();
        if replacement_rank != tier_rank {
            return Err(PolicyError::amendment_tier_mismatch());
        }
        assert(replacement.spec_tier().spec_rank() == tier.spec_rank());
        let proposal = Self {
            base_policy_id,
            successor_policy_id,
            tier,
            replacement,
            amendment_digest,
        };
        reveal(PolicyAmendmentProposal::spec_base_policy_id);
        reveal(PolicyAmendmentProposal::spec_successor_policy_id);
        reveal(PolicyAmendmentProposal::spec_tier);
        reveal(PolicyAmendmentProposal::spec_replacement);
        reveal(PolicyAmendmentProposal::spec_amendment_digest);
        Ok(proposal)
    }

    /// Returns the exact immutable base policy identity.
    #[must_use]
    pub const fn base_policy_id(&self) -> (policy_id: PolicyId)
        ensures
            policy_id == self.spec_base_policy_id_value(),
            policy_id.spec_bytes() == self.spec_base_policy_id(),
    { self.base_policy_id }

    /// Returns the proposed fresh successor policy identity.
    #[must_use]
    pub const fn successor_policy_id(&self) -> (policy_id: PolicyId)
        ensures
            policy_id == self.spec_successor_policy_id_value(),
            policy_id.spec_bytes() == self.spec_successor_policy_id(),
    { self.successor_policy_id }

    /// Returns the only ordinary restriction tier this proposal changes.
    #[must_use]
    pub const fn tier(&self) -> (tier: PolicyTier)
        ensures tier == self.spec_tier(),
    { self.tier }

    /// Returns the complete replacement restriction layer.
    #[must_use]
    pub const fn replacement(&self) -> (replacement: &RestrictionLayer)
        ensures *replacement == self.spec_replacement(),
    { &self.replacement }

    /// Returns the exact digest binding the amendment proposal.
    #[must_use]
    pub const fn amendment_digest(&self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_amendment_digest(),
    { self.amendment_digest }
}

} // verus!
