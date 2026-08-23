//! Verified extensional equality for opaque authority identities.

use peritus_types::{
    ActionId, BudgetId, BudgetReservationId, RevisionTuple, Sha256Digest,
};
use vstd::prelude::*;

verus! {

pub(super) open spec fn budget_ids_equal(left: BudgetId, right: BudgetId) -> bool {
    bytes_match(left.spec_bytes(), right.spec_bytes())
}

pub(super) proof fn budget_ids_symmetric(left: BudgetId, right: BudgetId)
    requires budget_ids_equal(left, right),
    ensures budget_ids_equal(right, left),
{
}

pub(super) proof fn budget_ids_transitive(
    first: BudgetId,
    second: BudgetId,
    third: BudgetId,
)
    requires
        budget_ids_equal(first, second),
        budget_ids_equal(second, third),
    ensures budget_ids_equal(first, third),
{
}

pub(super) open spec fn bytes_match(left: [u8; 16], right: [u8; 16]) -> bool {
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
}

pub(super) open spec fn parent_matches(parent: Option<BudgetId>, expected: BudgetId) -> bool {
    match parent {
        Some(actual) => budget_ids_equal(actual, expected),
        None => false,
    }
}

pub(super) open spec fn parents_equal(
    left: Option<BudgetId>,
    right: Option<BudgetId>,
) -> bool {
    match (left, right) {
        (Some(left_id), Some(right_id)) => budget_ids_equal(left_id, right_id),
        (None, None) => true,
        _ => false,
    }
}

pub(super) open spec fn reservation_ids_equal(
    left: BudgetReservationId,
    right: BudgetReservationId,
) -> bool {
    bytes_match(left.spec_bytes(), right.spec_bytes())
}

pub(super) open spec fn action_ids_equal(left: ActionId, right: ActionId) -> bool {
    bytes_match(left.spec_bytes(), right.spec_bytes())
}

pub(super) open spec fn digests_equal(left: Sha256Digest, right: Sha256Digest) -> bool {
    bytes32_match(left.spec_bytes(), right.spec_bytes())
}

pub(super) open spec fn revisions_equal(left: RevisionTuple, right: RevisionTuple) -> bool {
    bytes_match(
        left.spec_acceptance_spec_id().spec_bytes(),
        right.spec_acceptance_spec_id().spec_bytes(),
    )
        && bytes_match(
            left.spec_harness_id().spec_bytes(),
            right.spec_harness_id().spec_bytes(),
        )
        && bytes_match(
            left.spec_workspace_id().spec_bytes(),
            right.spec_workspace_id().spec_bytes(),
        )
        && left.spec_workspace_generation().spec_value()
            == right.spec_workspace_generation().spec_value()
        && left.spec_workspace_revision().spec_value()
            == right.spec_workspace_revision().spec_value()
        && bytes_match(
            left.spec_policy_id().spec_bytes(),
            right.spec_policy_id().spec_bytes(),
        )
        && bytes_match(
            left.spec_provider_profile_id().spec_bytes(),
            right.spec_provider_profile_id().spec_bytes(),
        )
}

pub(super) open spec fn bytes32_match(left: [u8; 32], right: [u8; 32]) -> bool {
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
        && left[16] == right[16]
        && left[17] == right[17]
        && left[18] == right[18]
        && left[19] == right[19]
        && left[20] == right[20]
        && left[21] == right[21]
        && left[22] == right[22]
        && left[23] == right[23]
        && left[24] == right[24]
        && left[25] == right[25]
        && left[26] == right[26]
        && left[27] == right[27]
        && left[28] == right[28]
        && left[29] == right[29]
        && left[30] == right[30]
        && left[31] == right[31]
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn budget_id_equal(left: BudgetId, right: BudgetId) -> (result: bool)
    ensures result == budget_ids_equal(left, right),
{
    bytes_equal(left.as_bytes(), right.as_bytes())
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn parent_equal(
    left: Option<BudgetId>,
    right: Option<BudgetId>,
) -> (result: bool)
    ensures result == parents_equal(left, right),
{
    match (left, right) {
        (Some(left_id), Some(right_id)) => budget_id_equal(left_id, right_id),
        (None, None) => true,
        _ => false,
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn parent_matches_id(
    parent: Option<BudgetId>,
    expected: BudgetId,
) -> (result: bool)
    ensures result == parent_matches(parent, expected),
{
    match parent {
        Some(actual) => budget_id_equal(actual, expected),
        None => false,
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn reservation_id_equal(
    left: BudgetReservationId,
    right: BudgetReservationId,
) -> (result: bool)
    ensures result == reservation_ids_equal(left, right),
{
    bytes_equal(left.as_bytes(), right.as_bytes())
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn action_id_equal(left: ActionId, right: ActionId) -> (result: bool)
    ensures result == action_ids_equal(left, right),
{
    bytes_equal(left.as_bytes(), right.as_bytes())
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn digest_equal(left: Sha256Digest, right: Sha256Digest) -> (result: bool)
    ensures result == digests_equal(left, right),
{
    bytes32_equal(left.as_bytes(), right.as_bytes())
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn optional_digest_equal(
    left: Option<Sha256Digest>,
    right: Option<Sha256Digest>,
) -> (result: bool)
    ensures result == crate::invariant::optional_digests_equal(left, right),
{
    match (left, right) {
        (Some(left_digest), Some(right_digest)) => digest_equal(left_digest, right_digest),
        (None, None) => true,
        _ => false,
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn revision_equal(left: RevisionTuple, right: RevisionTuple) -> (result: bool)
    ensures result == revisions_equal(left, right),
{
    bytes_equal(
        left.acceptance_spec_id().as_bytes(),
        right.acceptance_spec_id().as_bytes(),
    ) && bytes_equal(left.harness_id().as_bytes(), right.harness_id().as_bytes())
        && bytes_equal(left.workspace_id().as_bytes(), right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes_equal(left.policy_id().as_bytes(), right.policy_id().as_bytes())
        && bytes_equal(
            left.provider_profile_id().as_bytes(),
            right.provider_profile_id().as_bytes(),
        )
}

const fn bytes_equal(left: &[u8; 16], right: &[u8; 16]) -> (result: bool)
    ensures result == bytes_match(*left, *right),
{
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
}

const fn bytes32_equal(left: &[u8; 32], right: &[u8; 32]) -> (result: bool)
    ensures result == bytes32_match(*left, *right),
{
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
        && left[16] == right[16]
        && left[17] == right[17]
        && left[18] == right[18]
        && left[19] == right[19]
        && left[20] == right[20]
        && left[21] == right[21]
        && left[22] == right[22]
        && left[23] == right[23]
        && left[24] == right[24]
        && left[25] == right[25]
        && left[26] == right[26]
        && left[27] == right[27]
        && left[28] == right[28]
        && left[29] == right[29]
        && left[30] == right[30]
        && left[31] == right[31]
}

} // verus!
