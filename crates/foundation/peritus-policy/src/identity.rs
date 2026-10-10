//! Exact executable identity, revision, and canonical-order refinement helpers.

use crate::ActorRole;
use core::cmp::Ordering;
use peritus_types::{ActorId, EnvironmentId, RevisionTuple};

const fn compare_identifier_bytes_from(
    left: &[u8; 16],
    right: &[u8; 16],
    index: usize,
) -> Ordering {
    if index == left.len() {
        Ordering::Equal
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_identifier_bytes_from(left, right, index + 1)
    }
}
pub const fn compare_identifier_bytes(left: &[u8; 16], right: &[u8; 16]) -> Ordering {
    compare_identifier_bytes_from(left, right, 0)
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
pub const fn identifier_values_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    identifier_values_equal_from(left, right, 0)
}
pub fn actor_values_contain(values: &[ActorId], value: ActorId) -> bool {
    let target = *value.as_bytes();
    let mut index = 0;
    while index < values.len() {
        let candidate = *values[index].as_bytes();
        if identifier_values_equal(candidate, target) {
            return true;
        }
        index += 1;
    }
    false
}
pub fn environment_values_contain(values: &[EnvironmentId], value: EnvironmentId) -> bool {
    let target = *value.as_bytes();
    let mut index = 0;
    while index < values.len() {
        let candidate = *values[index].as_bytes();
        if identifier_values_equal(candidate, target) {
            return true;
        }
        index += 1;
    }
    false
}
pub fn role_values_contain(values: &[ActorRole], value: ActorRole) -> bool {
    let target = value.canonical_rank();
    let mut index = 0;
    while index < values.len() {
        if values[index].canonical_rank() == target {
            return true;
        }
        index += 1;
    }
    false
}
pub const fn revision_values_equal(left: RevisionTuple, right: RevisionTuple) -> bool {
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
