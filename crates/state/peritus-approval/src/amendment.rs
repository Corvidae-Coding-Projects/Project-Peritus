//! Exact signed and approved policy-amendment identity.

use peritus_policy::{PolicyRevisionCandidate, PolicyTier};
use peritus_types::{PolicyId, RevisionNumber, Sha256Digest};

const fn identifier_values_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}
const fn digest_values_equal_from(left: [u8; 32], right: [u8; 32], index: usize) -> bool {
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        digest_values_equal_from(left, right, index + 1)
    }
}
const fn tier_rank(tier: PolicyTier) -> u8 {
    match tier {
        PolicyTier::System => 0,
        PolicyTier::User => 1,
        PolicyTier::Project => 2,
        PolicyTier::Run => 3,
        PolicyTier::Session => 4,
        PolicyTier::RoleHarness => 5,
    }
}
#[doc = " Four independently signed fields identifying one proposed policy successor."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AmendmentIdentity {
    base_policy_id: PolicyId,
    successor_policy_id: PolicyId,
    tier: PolicyTier,
    amendment_digest: Sha256Digest,
}
impl AmendmentIdentity {
    #[doc = " Creates an exact amendment identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a binding error when the successor reuses the base identity."]
    pub fn new(
        base_policy_id: PolicyId,
        successor_policy_id: PolicyId,
        tier: PolicyTier,
        amendment_digest: Sha256Digest,
    ) -> Result<Self, crate::ApprovalError> {
        if base_policy_id == successor_policy_id {
            Err(crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Policy))
        } else {
            Ok(Self { base_policy_id, successor_policy_id, tier, amendment_digest })
        }
    }
    #[doc = " Returns the immutable base policy identity."]
    #[must_use]
    pub const fn base_policy_id(self) -> PolicyId {
        self.base_policy_id
    }
    #[doc = " Returns the fresh successor policy identity."]
    #[must_use]
    pub const fn successor_policy_id(self) -> PolicyId {
        self.successor_policy_id
    }
    #[doc = " Returns the exact replacement tier."]
    #[must_use]
    pub const fn tier(self) -> PolicyTier {
        self.tier
    }
    #[doc = " Returns the externally refined complete-replacement digest."]
    #[must_use]
    pub const fn amendment_digest(self) -> Sha256Digest {
        self.amendment_digest
    }
    #[doc = " Returns whether all four fields match one checked policy candidate."]
    #[must_use]
    pub const fn matches_candidate(&self, candidate: &PolicyRevisionCandidate) -> bool {
        let base_policy_id = candidate.base_policy_id();
        let successor_policy_id = candidate.successor_policy_id();
        let tier = candidate.tier();
        let amendment_digest = candidate.amendment_digest();

        identifier_values_equal_from(*self.base_policy_id.as_bytes(), *base_policy_id.as_bytes(), 0)
            && identifier_values_equal_from(
                *self.successor_policy_id.as_bytes(),
                *successor_policy_id.as_bytes(),
                0,
            )
            && tier_rank(self.tier) == tier_rank(tier)
            && digest_values_equal_from(
                *self.amendment_digest.as_bytes(),
                *amendment_digest.as_bytes(),
                0,
            )
    }
}
#[doc = " Opaque logical authorization for one exact previewed successor."]
#[doc = ""]
#[doc = " This value is move-only and is not an active-policy fact or durable receipt."]
#[doc = " Callers cannot construct or duplicate it directly:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_approval::{AmendmentIdentity, ApprovedPolicyAmendment};"]
#[doc = " use peritus_types::RevisionNumber;"]
#[doc = ""]
#[doc = " fn forge("]
#[doc = "     identity: AmendmentIdentity,"]
#[doc = "     registry_revision: RevisionNumber,"]
#[doc = " ) -> ApprovedPolicyAmendment {"]
#[doc = "     ApprovedPolicyAmendment { identity, registry_revision }"]
#[doc = " }"]
#[doc = " ```"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_approval::ApprovedPolicyAmendment;"]
#[doc = ""]
#[doc = " fn duplicate(value: ApprovedPolicyAmendment) {"]
#[doc = "     let _copy = value.clone();"]
#[doc = " }"]
#[doc = " ```"]
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovedPolicyAmendment {
    pub(crate) identity: AmendmentIdentity,
    pub(crate) registry_revision: RevisionNumber,
}
#[doc = " Successful move-only consumption of one amendment authorization."]
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalAmendmentOutcome {
    pub(crate) aggregate: crate::ApprovalAggregate,
    pub(crate) approval: ApprovedPolicyAmendment,
}
impl ApprovalAmendmentOutcome {
    #[doc = " Borrows the exact amended successor aggregate."]
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate {
        &self.aggregate
    }
    #[doc = " Borrows the unprivileged exact amendment approval."]
    #[must_use]
    pub const fn approval(&self) -> &ApprovedPolicyAmendment {
        &self.approval
    }
    #[doc = " Consumes the outcome into its move-only parts."]
    #[must_use]
    pub fn into_parts(self) -> (crate::ApprovalAggregate, ApprovedPolicyAmendment) {
        (self.aggregate, self.approval)
    }
}
impl ApprovedPolicyAmendment {
    pub(crate) const fn new(
        identity: AmendmentIdentity,
        registry_revision: RevisionNumber,
    ) -> Self {
        Self { identity, registry_revision }
    }
    #[doc = " Returns the four-field approved amendment identity."]
    #[must_use]
    pub const fn identity(&self) -> AmendmentIdentity {
        self.identity
    }
    #[doc = " Returns the exact non-authoritative credential snapshot revision used for authentication."]
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber {
        self.registry_revision
    }
    #[doc = " Returns whether this authorization exactly matches a previewed candidate."]
    #[must_use]
    pub const fn matches_candidate(&self, candidate: &PolicyRevisionCandidate) -> bool {
        self.identity.matches_candidate(candidate)
    }
}
