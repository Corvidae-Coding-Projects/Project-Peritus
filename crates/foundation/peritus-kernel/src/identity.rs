//! Verified extensional equality for opaque lifecycle identities.

use peritus_types::{EventId, RevisionTuple, RunId};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn bytes_match(left: [u8; 16], right: [u8; 16]) -> bool {
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

pub(crate) open spec fn run_ids_equal(left: RunId, right: RunId) -> bool {
    bytes_match(left.spec_bytes(), right.spec_bytes())
}

pub(crate) open spec fn event_ids_equal(left: EventId, right: EventId) -> bool {
    bytes_match(left.spec_bytes(), right.spec_bytes())
}

pub(crate) open spec fn optional_event_ids_equal(
    left: Option<EventId>,
    right: Option<EventId>,
) -> bool {
    match (left, right) {
        (Some(left_id), Some(right_id)) => event_ids_equal(left_id, right_id),
        (None, None) => true,
        _ => false,
    }
}

pub(crate) open spec fn revisions_equal(left: RevisionTuple, right: RevisionTuple) -> bool {
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

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn run_id_equal(left: RunId, right: RunId) -> (result: bool)
    ensures result == run_ids_equal(left, right),
{
    bytes_equal(left.as_bytes(), right.as_bytes())
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable contract"
)]
pub(crate) const fn optional_event_id_equal(
    left: Option<EventId>,
    right: Option<EventId>,
) -> (result: bool)
    ensures result == optional_event_ids_equal(left, right),
{
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
pub(crate) const fn revision_equal(
    left: RevisionTuple,
    right: RevisionTuple,
) -> (result: bool)
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

} // verus!
