//! Executable refinement of exact revision and acceptance-specification identity equality.

use peritus_types::{AcceptanceSpecId, RevisionTuple};
use vstd::prelude::*;

verus! {

/// Compares all bytes of the two content-addressed evidence requirement identities.
pub const fn evidence_requirement_matches(
    left: peritus_spec::EvidenceRequirementId,
    right: peritus_spec::EvidenceRequirementId,
) -> (equal: bool)
    ensures equal == crate::model::evidence_requirement_matches(left, right),
{
    digest_matches(left.digest(), right.digest())
}

/// Compares every byte of two supplied digest values.
pub const fn digest_matches(
    left: peritus_types::Sha256Digest,
    right: peritus_types::Sha256Digest,
) -> (equal: bool)
    ensures equal == crate::model::digests_match(left, right),
{
    let left_bytes = left.into_bytes();
    let right_bytes = right.into_bytes();
    let mut index = 0;
    while index < 32
        invariant
            index <= 32,
            left_bytes == left.spec_bytes(),
            right_bytes == right.spec_bytes(),
            forall |prior: int| 0 <= prior < index ==> left_bytes[prior] == right_bytes[prior],
        decreases 32 - index,
    {
        if left_bytes[index] != right_bytes[index] {
            assert(left.spec_bytes()[index as int] != right.spec_bytes()[index as int]);
            return false;
        }
        index += 1;
    }
    true
}

/// Compares the complete content-addressed review category identities.
pub const fn review_category_matches(
    left: peritus_spec::ReviewCategory,
    right: peritus_spec::ReviewCategory,
) -> (equal: bool)
    ensures equal == crate::model::review_categories_match(left, right),
{
    digest_matches(left.digest(), right.digest())
}

/// Compares the remaining bytes of two identifiers from the supplied index.
pub open spec fn same_identifier_from(
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

/// Compares all sixteen bytes of two identifiers.
pub open spec fn same_identifier(left: [u8; 16], right: [u8; 16]) -> bool {
    same_identifier_from(left, right, 0)
}

const fn identifier_values_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (equal: bool)
    requires index <= 16,
    ensures equal == same_identifier_from(left, right, index as nat),
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

const fn identifier_values_equal(left: [u8; 16], right: [u8; 16]) -> (equal: bool)
    ensures equal == same_identifier(left, right),
{
    identifier_values_equal_from(left, right, 0)
}

/// Compares every byte of the two gate identities.
pub const fn gate_id_matches(
    left: peritus_types::GateId,
    right: peritus_types::GateId,
) -> (equal: bool)
    ensures equal == crate::model::gate_ids_match(left, right),
{
    identifier_values_equal(*left.as_bytes(), *right.as_bytes())
}

/// Compares all bytes of two reviewer actor identities.
pub const fn reviewer_actor_matches(
    left: peritus_types::ActorId,
    right: peritus_types::ActorId,
) -> (equal: bool)
    ensures equal == crate::model::reviewer_actors_match(left, right),
{
    identifier_values_equal(*left.as_bytes(), *right.as_bytes())
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling evaluators require the executable identity refinement"
)]
pub(crate) const fn acceptance_id_matches(
    left: AcceptanceSpecId,
    right: AcceptanceSpecId,
) -> (matches: bool)
    ensures matches == peritus_spec::acceptance_ids_match(left, right),
{
    let matches = identifier_values_equal(*left.as_bytes(), *right.as_bytes());
    proof {
        reveal_with_fuel(same_identifier_from, 17);
    }
    matches
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling evaluators require the executable revision refinement"
)]
pub(crate) const fn revision_matches(
    left: RevisionTuple,
    right: RevisionTuple,
) -> (matches: bool)
    ensures matches == crate::model::revision_fresh(left, right),
{
    identifier_values_equal(
        *left.acceptance_spec_id().as_bytes(),
        *right.acceptance_spec_id().as_bytes(),
    )
        && identifier_values_equal(*left.harness_id().as_bytes(), *right.harness_id().as_bytes())
        && identifier_values_equal(
            *left.workspace_id().as_bytes(),
            *right.workspace_id().as_bytes(),
        )
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && identifier_values_equal(*left.policy_id().as_bytes(), *right.policy_id().as_bytes())
        && identifier_values_equal(
            *left.provider_profile_id().as_bytes(),
            *right.provider_profile_id().as_bytes(),
        )
}

} // verus!
