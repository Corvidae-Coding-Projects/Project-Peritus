//! Executable refinement of exact revision and acceptance-specification identity equality.

use peritus_types::{AcceptanceSpecId, RevisionTuple};
use vstd::prelude::*;

verus! {

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
