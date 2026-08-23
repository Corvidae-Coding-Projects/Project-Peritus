//! Exact signed and approved policy-amendment identity.

use peritus_policy::{PolicyRevisionCandidate, PolicyTier};
use peritus_types::{PolicyId, RevisionNumber, Sha256Digest};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn same_identifier_from(
    left: [u8; 16],
    right: [u8; 16],
    index: nat,
) -> bool
    decreases 16 - index,
{
    if index >= 16 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_identifier_from(left, right, index + 1)
    }
}

pub(crate) open spec fn same_digest_from(
    left: [u8; 32],
    right: [u8; 32],
    index: nat,
) -> bool
    decreases 32 - index,
{
    if index >= 32 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_digest_from(left, right, index + 1)
    }
}

const fn identifier_values_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (result: bool)
    requires index <= 16,
    ensures result == same_identifier_from(left, right, index as nat),
    decreases 16 - index,
{
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}

const fn digest_values_equal_from(
    left: [u8; 32],
    right: [u8; 32],
    index: usize,
) -> (result: bool)
    requires index <= 32,
    ensures result == same_digest_from(left, right, index as nat),
    decreases 32 - index,
{
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        digest_values_equal_from(left, right, index + 1)
    }
}

const fn tier_rank(tier: PolicyTier) -> (rank: u8)
    ensures rank as int == tier.spec_rank(),
{
    match tier {
        PolicyTier::System => 0,
        PolicyTier::User => 1,
        PolicyTier::Project => 2,
        PolicyTier::Run => 3,
        PolicyTier::Session => 4,
        PolicyTier::RoleHarness => 5,
    }
}

/// Four independently signed fields identifying one proposed policy successor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AmendmentIdentity {
    base_policy_id: PolicyId,
    successor_policy_id: PolicyId,
    tier: PolicyTier,
    amendment_digest: Sha256Digest,
}

impl AmendmentIdentity {
    /// Returns exact four-field equality with one checked policy candidate.
    pub closed spec fn spec_matches_candidate(
        &self,
        candidate: &PolicyRevisionCandidate,
    ) -> bool {
        same_identifier_from(
            self.base_policy_id.spec_bytes(),
            candidate.spec_base_policy_id_value().spec_bytes(),
            0,
        ) && same_identifier_from(
            self.successor_policy_id.spec_bytes(),
            candidate.spec_successor_policy_id_value().spec_bytes(),
            0,
        ) && self.tier.spec_rank() == candidate.spec_tier().spec_rank()
            && same_digest_from(
                self.amendment_digest.spec_bytes(),
                candidate.spec_amendment_digest_value().spec_bytes(),
                0,
            )
    }

    /// Creates an exact amendment identity.
    ///
    /// # Errors
    ///
    /// Returns a binding error when the successor reuses the base identity.
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

    /// Returns the immutable base policy identity.
    #[must_use]
    pub const fn base_policy_id(self) -> PolicyId { self.base_policy_id }

    /// Returns the fresh successor policy identity.
    #[must_use]
    pub const fn successor_policy_id(self) -> PolicyId { self.successor_policy_id }

    /// Returns the exact replacement tier.
    #[must_use]
    pub const fn tier(self) -> PolicyTier { self.tier }

    /// Returns the externally refined complete-replacement digest.
    #[must_use]
    pub const fn amendment_digest(self) -> Sha256Digest { self.amendment_digest }

    /// Returns whether all four fields match one checked policy candidate.
    #[must_use]
    pub const fn matches_candidate(&self, candidate: &PolicyRevisionCandidate) -> (result: bool)
        ensures result == self.spec_matches_candidate(candidate),
    {
        let base_policy_id = candidate.base_policy_id();
        let successor_policy_id = candidate.successor_policy_id();
        let tier = candidate.tier();
        let amendment_digest = candidate.amendment_digest();
        let result = identifier_values_equal_from(
                *self.base_policy_id.as_bytes(), *base_policy_id.as_bytes(), 0,
            )
            && identifier_values_equal_from(
                *self.successor_policy_id.as_bytes(), *successor_policy_id.as_bytes(), 0,
            )
            && tier_rank(self.tier) == tier_rank(tier)
            && digest_values_equal_from(
                *self.amendment_digest.as_bytes(), *amendment_digest.as_bytes(), 0,
            );
        reveal(AmendmentIdentity::spec_matches_candidate);
        result
    }
}

/// Opaque logical authorization for one exact previewed successor.
///
/// This value is move-only and is not an active-policy fact or durable receipt.
/// Callers cannot construct or duplicate it directly:
///
/// ```compile_fail
/// use peritus_approval::{AmendmentIdentity, ApprovedPolicyAmendment};
/// use peritus_types::RevisionNumber;
///
/// fn forge(
///     identity: AmendmentIdentity,
///     registry_revision: RevisionNumber,
/// ) -> ApprovedPolicyAmendment {
///     ApprovedPolicyAmendment { identity, registry_revision }
/// }
/// ```
///
/// ```compile_fail
/// use peritus_approval::ApprovedPolicyAmendment;
///
/// fn duplicate(value: ApprovedPolicyAmendment) {
///     let _copy = value.clone();
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovedPolicyAmendment {
    pub(crate) identity: AmendmentIdentity,
    pub(crate) registry_revision: RevisionNumber,
}

/// Successful move-only consumption of one amendment authorization.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalAmendmentOutcome {
    pub(crate) aggregate: crate::ApprovalAggregate,
    pub(crate) approval: ApprovedPolicyAmendment,
}

impl ApprovalAmendmentOutcome {
    /// Returns the accepted aggregate's closed logical model projection.
    pub closed spec fn spec_model(&self) -> crate::model::ApprovalModelState {
        self.aggregate.spec_model()
    }

    pub(crate) proof fn prove_model(&self)
        ensures self.spec_model() == self.aggregate.spec_model(),
    {
    }

    /// Borrows the exact amended successor aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate { &self.aggregate }

    /// Borrows the unprivileged exact amendment approval.
    #[must_use]
    pub const fn approval(&self) -> &ApprovedPolicyAmendment { &self.approval }

    /// Consumes the outcome into its move-only parts.
    #[must_use]
    pub fn into_parts(self) -> (crate::ApprovalAggregate, ApprovedPolicyAmendment) {
        (self.aggregate, self.approval)
    }
}

impl ApprovedPolicyAmendment {
    pub(crate) const fn new(
        identity: AmendmentIdentity,
        registry_revision: RevisionNumber,
    ) -> (approval: Self)
        ensures
            approval.identity == identity,
            approval.registry_revision == registry_revision,
    {
        Self { identity, registry_revision }
    }

    /// Returns the four-field approved amendment identity.
    #[must_use]
    pub const fn identity(&self) -> AmendmentIdentity { self.identity }

    /// Returns the exact non-authoritative credential snapshot revision used for authentication.
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber { self.registry_revision }

    /// Returns whether this authorization exactly matches a previewed candidate.
    #[must_use]
    pub const fn matches_candidate(&self, candidate: &PolicyRevisionCandidate) -> bool {
        self.identity.matches_candidate(candidate)
    }
}

} // verus!
