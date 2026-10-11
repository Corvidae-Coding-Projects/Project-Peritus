//! Executable evidence rules proved with Verus and reused by ordinary Rust.

use peritus_types::RevisionTuple;

fn bytes16_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes16_equal_from(left, right, index + 1)
    }
}
fn bytes16_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    bytes16_equal_from(left, right, 0)
}
#[doc = " Checks exact equality of every revision component."]
#[must_use]
pub fn revisions_equal(left: &RevisionTuple, right: &RevisionTuple) -> bool {
    bytes16_equal(*left.acceptance_spec_id().as_bytes(), *right.acceptance_spec_id().as_bytes())
        && bytes16_equal(*left.harness_id().as_bytes(), *right.harness_id().as_bytes())
        && bytes16_equal(*left.workspace_id().as_bytes(), *right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes16_equal(*left.policy_id().as_bytes(), *right.policy_id().as_bytes())
        && bytes16_equal(
            *left.provider_profile_id().as_bytes(),
            *right.provider_profile_id().as_bytes(),
        )
}
#[doc = " Checks that a cause is strictly older than its child."]
#[must_use]
pub const fn causal_position(parent: u64, child: u64) -> bool {
    parent > 0 && parent < child
}
#[doc = " Checks the fixed canonical bundle section transition."]
#[must_use]
pub const fn bundle_section_transition(previous: u8, current: u8) -> bool {
    previous < 3 && current == previous + 1
}
#[doc = " Checks the nonempty frame coverage and all portable bundle collection bounds."]
#[must_use]
pub const fn bundle_plan_shape(records: u64, frames: u64, artifacts: u64, limit: u64) -> bool {
    records > 0
        && frames > 0
        && frames <= records
        && records <= limit
        && frames <= limit
        && artifacts <= limit
}
