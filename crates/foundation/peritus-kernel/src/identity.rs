//! Verified extensional equality for opaque lifecycle identities.

#![allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for cross-module executable contracts"
)]

use peritus_types::{EventId, FindingId, ReviewCycleId, RevisionTuple, RunId};
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

pub(crate) open spec fn review_cycle_ids_equal(
    left: ReviewCycleId,
    right: ReviewCycleId,
) -> bool {
    left.spec_bytes()@ == right.spec_bytes()@
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
    ensures
        result == run_ids_equal(left, right),
        result == (left.spec_bytes()@ == right.spec_bytes()@),
{
    identifier_equal(left.as_bytes(), right.as_bytes())
}

pub(crate) const fn finding_id_equal(left: FindingId, right: FindingId) -> (result: bool)
    ensures result == peritus_quality_policy::finding_ids_match(left, right),
{
    identifier_equal(left.as_bytes(), right.as_bytes())
}

pub(crate) const fn review_cycle_id_equal(
    left: ReviewCycleId,
    right: ReviewCycleId,
) -> (result: bool)
    ensures result == review_cycle_ids_equal(left, right),
{
    identifier_equal(left.as_bytes(), right.as_bytes())
}

const fn identifier_equal(left: &[u8; 16], right: &[u8; 16]) -> (result: bool)
    ensures
        result == (left@ == right@),
        result == bytes_match(*left, *right),
{
    let mut index = 0;
    while index < 16
        invariant
            index <= 16,
            forall |prior: int| 0 <= prior < index ==> left@[prior] == right@[prior],
        decreases 16 - index,
    {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    assert(left@ =~= right@);
    true
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
    ensures
        result == revisions_equal(left, right),
        result == peritus_quality_policy::revision_fresh(left, right),
{
    let result = bytes_equal(
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
        );
    proof {
        bytes_match_is_same_identifier(
            left.spec_acceptance_spec_id().spec_bytes(),
            right.spec_acceptance_spec_id().spec_bytes(),
        );
        bytes_match_is_same_identifier(
            left.spec_harness_id().spec_bytes(),
            right.spec_harness_id().spec_bytes(),
        );
        bytes_match_is_same_identifier(
            left.spec_workspace_id().spec_bytes(),
            right.spec_workspace_id().spec_bytes(),
        );
        bytes_match_is_same_identifier(
            left.spec_policy_id().spec_bytes(),
            right.spec_policy_id().spec_bytes(),
        );
        bytes_match_is_same_identifier(
            left.spec_provider_profile_id().spec_bytes(),
            right.spec_provider_profile_id().spec_bytes(),
        );
    }
    result
}

proof fn bytes_match_is_same_identifier(left: [u8; 16], right: [u8; 16])
    ensures bytes_match(left, right) == peritus_quality_policy::same_identifier(left, right),
{
    reveal_with_fuel(peritus_quality_policy::same_identifier_from, 17);
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
