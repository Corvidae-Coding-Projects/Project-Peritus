//! Verified extensional equality for opaque lifecycle identities.

#![allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for cross-module executable contracts"
)]

use peritus_types::{EventId, FindingId, ReviewCycleId, RevisionTuple, RunId};

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn run_id_equal(left: RunId, right: RunId) -> bool {
    identifier_equal(left.as_bytes(), right.as_bytes())
}
pub(crate) const fn finding_id_equal(left: FindingId, right: FindingId) -> bool {
    identifier_equal(left.as_bytes(), right.as_bytes())
}
pub(crate) const fn review_cycle_id_equal(left: ReviewCycleId, right: ReviewCycleId) -> bool {
    identifier_equal(left.as_bytes(), right.as_bytes())
}
const fn identifier_equal(left: &[u8; 16], right: &[u8; 16]) -> bool {
    let mut index = 0;
    while index < 16 {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn optional_event_id_equal(left: Option<EventId>, right: Option<EventId>) -> bool {
    match (left, right) {
        (Some(left_id), Some(right_id)) => bytes_equal(left_id.as_bytes(), right_id.as_bytes()),
        (None, None) => true,
        _ => false,
    }
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn revision_equal(left: RevisionTuple, right: RevisionTuple) -> bool {
    bytes_equal(left.acceptance_spec_id().as_bytes(), right.acceptance_spec_id().as_bytes())
        && bytes_equal(left.harness_id().as_bytes(), right.harness_id().as_bytes())
        && bytes_equal(left.workspace_id().as_bytes(), right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes_equal(left.policy_id().as_bytes(), right.policy_id().as_bytes())
        && bytes_equal(
            left.provider_profile_id().as_bytes(),
            right.provider_profile_id().as_bytes(),
        )
}
const fn bytes_equal(left: &[u8; 16], right: &[u8; 16]) -> bool {
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
