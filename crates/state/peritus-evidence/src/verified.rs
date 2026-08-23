//! Executable evidence rules proved with Verus and reused by ordinary Rust.

use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

closed spec fn bytes16_match_from(
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
            && bytes16_match_from(left, right, index + 1)
    }
}

closed spec fn bytes16_match(left: [u8; 16], right: [u8; 16]) -> bool {
    bytes16_match_from(left, right, 0)
}

fn bytes16_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (equal: bool)
    requires index <= 16,
    ensures equal == bytes16_match_from(left, right, index as nat),
    decreases 16 - index,
{
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes16_equal_from(left, right, index + 1)
    }
}

fn bytes16_equal(left: [u8; 16], right: [u8; 16]) -> (equal: bool)
    ensures equal == bytes16_match(left, right),
{
    bytes16_equal_from(left, right, 0)
}

/// Specification-level exact equality of all revision identity fields.
pub closed spec fn revision_fields_match(left: &RevisionTuple, right: &RevisionTuple) -> bool {
    bytes16_match(
        left.spec_acceptance_spec_id().spec_bytes(),
        right.spec_acceptance_spec_id().spec_bytes(),
    ) && bytes16_match(
        left.spec_harness_id().spec_bytes(),
        right.spec_harness_id().spec_bytes(),
    ) && bytes16_match(
        left.spec_workspace_id().spec_bytes(),
        right.spec_workspace_id().spec_bytes(),
    ) && left.spec_workspace_generation().spec_value()
        == right.spec_workspace_generation().spec_value()
        && left.spec_workspace_revision().spec_value()
            == right.spec_workspace_revision().spec_value()
        && bytes16_match(
            left.spec_policy_id().spec_bytes(),
            right.spec_policy_id().spec_bytes(),
        )
        && bytes16_match(
            left.spec_provider_profile_id().spec_bytes(),
            right.spec_provider_profile_id().spec_bytes(),
        )
}

/// Checks exact equality of every revision component.
#[must_use]
pub fn revisions_equal(left: &RevisionTuple, right: &RevisionTuple) -> (equal: bool)
    ensures equal == revision_fields_match(left, right)
{
    bytes16_equal(
        *left.acceptance_spec_id().as_bytes(),
        *right.acceptance_spec_id().as_bytes(),
    ) && bytes16_equal(*left.harness_id().as_bytes(), *right.harness_id().as_bytes())
        && bytes16_equal(*left.workspace_id().as_bytes(), *right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes16_equal(*left.policy_id().as_bytes(), *right.policy_id().as_bytes())
        && bytes16_equal(
            *left.provider_profile_id().as_bytes(),
            *right.provider_profile_id().as_bytes(),
        )
}

/// Checks that a cause is strictly older than its child.
#[must_use]
pub const fn causal_position(parent: u64, child: u64) -> (valid: bool)
    ensures valid == (parent > 0 && parent < child)
{
    parent > 0 && parent < child
}

/// Checks the fixed canonical bundle section transition.
#[must_use]
pub const fn bundle_section_transition(previous: u8, current: u8) -> (valid: bool)
    ensures valid == (previous < 3 && current == previous + 1)
{
    previous < 3 && current == previous + 1
}

/// Checks the nonempty frame coverage and all portable bundle collection bounds.
#[must_use]
pub const fn bundle_plan_shape(records: u64, frames: u64, artifacts: u64, limit: u64) -> (valid: bool)
    ensures valid == (
        records > 0
            && frames > 0
            && frames <= records
            && records <= limit
            && frames <= limit
            && artifacts <= limit
    )
{
    records > 0
        && frames > 0
        && frames <= records
        && records <= limit
        && frames <= limit
        && artifacts <= limit
}

} // verus!
