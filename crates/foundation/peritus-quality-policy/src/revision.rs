//! Executable refinement of exact revision and acceptance-specification identity equality.

use peritus_types::{AcceptanceSpecId, RevisionTuple};

#[doc = " Compares all bytes of the two content-addressed evidence requirement identities."]
pub const fn evidence_requirement_matches(
    left: peritus_spec::EvidenceRequirementId,
    right: peritus_spec::EvidenceRequirementId,
) -> bool {
    digest_matches(left.digest(), right.digest())
}
#[doc = " Compares every byte of two supplied digest values."]
pub const fn digest_matches(
    left: peritus_types::Sha256Digest,
    right: peritus_types::Sha256Digest,
) -> bool {
    let left_bytes = left.into_bytes();
    let right_bytes = right.into_bytes();
    let mut index = 0;
    while index < 32 {
        if left_bytes[index] != right_bytes[index] {
            return false;
        }
        index += 1;
    }
    true
}
#[doc = " Compares the complete content-addressed review category identities."]
pub const fn review_category_matches(
    left: peritus_spec::ReviewCategory,
    right: peritus_spec::ReviewCategory,
) -> bool {
    digest_matches(left.digest(), right.digest())
}
const fn identifier_values_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}
const fn identifier_values_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    identifier_values_equal_from(left, right, 0)
}
#[doc = " Compares every byte of the two gate identities."]
pub const fn gate_id_matches(left: peritus_types::GateId, right: peritus_types::GateId) -> bool {
    identifier_values_equal(*left.as_bytes(), *right.as_bytes())
}
#[doc = " Compares all bytes of two reviewer actor identities."]
pub const fn reviewer_actor_matches(
    left: peritus_types::ActorId,
    right: peritus_types::ActorId,
) -> bool {
    identifier_values_equal(*left.as_bytes(), *right.as_bytes())
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling evaluators require the executable identity refinement"
)]
pub(crate) const fn acceptance_id_matches(left: AcceptanceSpecId, right: AcceptanceSpecId) -> bool {
    let matches = identifier_values_equal(*left.as_bytes(), *right.as_bytes());
    matches
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling evaluators require the executable revision refinement"
)]
pub(crate) const fn revision_matches(left: RevisionTuple, right: RevisionTuple) -> bool {
    identifier_values_equal(
        *left.acceptance_spec_id().as_bytes(),
        *right.acceptance_spec_id().as_bytes(),
    ) && identifier_values_equal(*left.harness_id().as_bytes(), *right.harness_id().as_bytes())
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
