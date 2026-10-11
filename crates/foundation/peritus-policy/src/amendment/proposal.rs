//! Exact checked amendment-proposal construction and projections.

use super::PolicyAmendmentProposal;
use crate::{PolicyError, PolicyTier, RestrictionLayer};
use peritus_types::{PolicyId, Sha256Digest};

impl PolicyAmendmentProposal {
    #[doc = " Creates an exact single-tier amendment proposal."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed failure if the successor reuses the base identity or the replacement layer"]
    #[doc = " does not have the declared tier."]
    pub fn new(
        base_policy_id: PolicyId,
        successor_policy_id: PolicyId,
        tier: PolicyTier,
        replacement: RestrictionLayer,
        amendment_digest: Sha256Digest,
    ) -> Result<Self, PolicyError> {
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
        let proposal =
            Self { base_policy_id, successor_policy_id, tier, replacement, amendment_digest };
        Ok(proposal)
    }
    #[doc = " Returns the exact immutable base policy identity."]
    #[must_use]
    pub const fn base_policy_id(&self) -> PolicyId {
        self.base_policy_id
    }
    #[doc = " Returns the proposed fresh successor policy identity."]
    #[must_use]
    pub const fn successor_policy_id(&self) -> PolicyId {
        self.successor_policy_id
    }
    #[doc = " Returns the only ordinary restriction tier this proposal changes."]
    #[must_use]
    pub const fn tier(&self) -> PolicyTier {
        self.tier
    }
    #[doc = " Returns the complete replacement restriction layer."]
    #[must_use]
    pub const fn replacement(&self) -> &RestrictionLayer {
        &self.replacement
    }
    #[doc = " Returns the exact digest binding the amendment proposal."]
    #[must_use]
    pub const fn amendment_digest(&self) -> Sha256Digest {
        self.amendment_digest
    }
}
