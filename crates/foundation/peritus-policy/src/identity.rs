//! Exact executable identity, revision, and canonical-order refinement helpers.

use crate::ActorRole;
use core::cmp::Ordering;
use peritus_types::{ActorId, EnvironmentId, RevisionTuple};
use vstd::prelude::*;

verus! {

const fn compare_identifier_bytes_from(
    left: &[u8; 16],
    right: &[u8; 16],
    index: usize,
) -> (result: Ordering)
    requires index <= left.len(), index <= right.len(),
    ensures result == peritus_types::canonical_byte_order_from(left@, right@, index as nat),
    decreases left.len() - index,
{
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

pub const fn compare_identifier_bytes(
    left: &[u8; 16],
    right: &[u8; 16],
) -> (result: Ordering)
    ensures result == peritus_types::canonical_byte_order_from(left@, right@, 0),
{
    compare_identifier_bytes_from(left, right, 0)
}

const fn identifier_values_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (result: bool)
    requires index <= 16,
    ensures result == crate::model::same_identifier_from(left, right, index as nat),
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

pub const fn identifier_values_equal(
    left: [u8; 16],
    right: [u8; 16],
) -> (result: bool)
    ensures result == crate::model::same_identifier(left, right),
{
    identifier_values_equal_from(left, right, 0)
}

pub open spec fn actor_values_spec_contains(values: Seq<ActorId>, value: ActorId) -> bool {
    exists |index: int| 0 <= index < values.len()
        && #[trigger] crate::model::same_identifier(
            values[index].spec_bytes(),
            value.spec_bytes(),
        )
}

pub fn actor_values_contain(values: &[ActorId], value: ActorId) -> (result: bool)
    ensures result == actor_values_spec_contains(values@, value),
{
    let target = *value.as_bytes();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            target == value.spec_bytes(),
            forall |prior: int| 0 <= prior < index ==>
                !#[trigger] crate::model::same_identifier(
                    values@[prior].spec_bytes(),
                    value.spec_bytes(),
                ),
        decreases values.len() - index,
    {
        let candidate = *values[index].as_bytes();
        if identifier_values_equal(candidate, target) {
            assert(actor_values_spec_contains(values@, value)) by {
                assert(exists |found: int| found == index && 0 <= found < values@.len()
                    && #[trigger] crate::model::same_identifier(
                        values@[found].spec_bytes(),
                        value.spec_bytes(),
                    ));
            }
            return true;
        }
        index += 1;
    }
    false
}

pub open spec fn environment_values_spec_contains(
    values: Seq<EnvironmentId>,
    value: EnvironmentId,
) -> bool {
    exists |index: int| 0 <= index < values.len()
        && #[trigger] crate::model::same_identifier(
            values[index].spec_bytes(),
            value.spec_bytes(),
        )
}

pub fn environment_values_contain(
    values: &[EnvironmentId],
    value: EnvironmentId,
) -> (result: bool)
    ensures result == environment_values_spec_contains(values@, value),
{
    let target = *value.as_bytes();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            target == value.spec_bytes(),
            forall |prior: int| 0 <= prior < index ==>
                !#[trigger] crate::model::same_identifier(
                    values@[prior].spec_bytes(),
                    value.spec_bytes(),
                ),
        decreases values.len() - index,
    {
        let candidate = *values[index].as_bytes();
        if identifier_values_equal(candidate, target) {
            assert(environment_values_spec_contains(values@, value)) by {
                assert(exists |found: int| found == index && 0 <= found < values@.len()
                    && #[trigger] crate::model::same_identifier(
                        values@[found].spec_bytes(),
                        value.spec_bytes(),
                    ));
            }
            return true;
        }
        index += 1;
    }
    false
}

pub open spec fn role_values_spec_contains(values: Seq<ActorRole>, value: ActorRole) -> bool {
    exists |index: int| 0 <= index < values.len()
        && #[trigger] values[index].spec_rank() == value.spec_rank()
}

pub fn role_values_contain(values: &[ActorRole], value: ActorRole) -> (result: bool)
    ensures result == role_values_spec_contains(values@, value),
{
    let target = value.canonical_rank();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            target as int == value.spec_rank(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] values@[prior].spec_rank() != value.spec_rank(),
        decreases values.len() - index,
    {
        if values[index].canonical_rank() == target {
            assert(role_values_spec_contains(values@, value)) by {
                assert(exists |found: int| found == index && 0 <= found < values@.len()
                    && #[trigger] values@[found].spec_rank() == value.spec_rank());
            }
            return true;
        }
        index += 1;
    }
    false
}

pub const fn revision_values_equal(
    left: RevisionTuple,
    right: RevisionTuple,
) -> (result: bool)
    ensures result == crate::model::same_revision(left, right),
{
    identifier_values_equal(*left.acceptance_spec_id().as_bytes(), *right.acceptance_spec_id().as_bytes())
        && identifier_values_equal(*left.harness_id().as_bytes(), *right.harness_id().as_bytes())
        && identifier_values_equal(*left.workspace_id().as_bytes(), *right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && identifier_values_equal(*left.policy_id().as_bytes(), *right.policy_id().as_bytes())
        && identifier_values_equal(
            *left.provider_profile_id().as_bytes(),
            *right.provider_profile_id().as_bytes(),
        )
}

} // verus!
