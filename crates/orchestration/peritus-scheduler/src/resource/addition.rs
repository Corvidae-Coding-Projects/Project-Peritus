//! Verified canonical resource addition used by public arithmetic and capacity aggregation.

use vstd::prelude::*;

#[cfg(verus_only)]
use super::ResourceKind;
use super::{ResourceEntry, ResourceQuantity, ResourceVector, VectorAddition};

mod merge;
mod proofs;

verus! {

/// Some shared dimension overflows the stored quantity width.
pub(super) open spec fn addition_overflows(
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
) -> bool {
    exists |pair: (int, int)| #![trigger left[pair.0], right[pair.1]]
        0 <= pair.0 < left.len()
            && 0 <= pair.1 < right.len()
            && left[pair.0].spec_kind() == right[pair.1].spec_kind()
            && left[pair.0].spec_quantity().spec_value() as int
                + right[pair.1].spec_quantity().spec_value() as int
                > u64::MAX as int
}

/// Exact canonical merge produced by addition, including explicit zero quantities.
pub(super) open spec fn added_entries(
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
) -> Seq<ResourceEntry>
    decreases left.len() + right.len(),
{
    if left.len() == 0 {
        right
    } else if right.len() == 0 {
        left
    } else if left.last().spec_kind().spec_tag() == right.last().spec_kind().spec_tag() {
        let quantity = left.last().spec_quantity().spec_value() as int
            + right.last().spec_quantity().spec_value() as int;
        added_entries(left.drop_last(), right.drop_last()).push(ResourceEntry {
            kind: left.last().spec_kind(),
            quantity: ResourceQuantity(if quantity <= u64::MAX as int {
                quantity as u64
            } else {
                0
            }),
        })
    } else if left.last().spec_kind().spec_tag() > right.last().spec_kind().spec_tag() {
        added_entries(left.drop_last(), right).push(left.last())
    } else {
        added_entries(left, right.drop_last()).push(right.last())
    }
}

/// Number of distinct dimensions in two canonical sequences.
pub(super) open spec fn union_dimensions(
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
) -> nat {
    added_entries(left, right).len()
}

/// Exact public addition admission.
pub(super) open spec fn addition_admissible(
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
    maximum_dimensions: u16,
) -> bool {
    !addition_overflows(left, right)
        && union_dimensions(left, right) <= maximum_dimensions as int
}

/// Strict kind ordering for a possibly empty intermediate merge.
pub(super) open spec fn entries_ordered(entries: Seq<ResourceEntry>) -> bool {
    forall |index: int| #![trigger entries[index]] 1 <= index < entries.len() ==>
        entries[index - 1].spec_kind().spec_tag() < entries[index].spec_kind().spec_tag()
}

/// Exact canonical entries represented by a successful addition.
pub(super) open spec fn addition_result(
    result: &ResourceVector,
    left: &ResourceVector,
    right: &ResourceVector,
) -> bool {
    result.spec_entries() == added_entries(left.spec_entries(), right.spec_entries())
}

/// Exact quantity sum represented by a successful addition.
pub(super) open spec fn addition_quantities(
    result: &ResourceVector,
    left: &ResourceVector,
    right: &ResourceVector,
) -> bool {
    forall |kind: ResourceKind| #![auto]
        result.spec_quantity(kind)
            == left.spec_quantity(kind) + right.spec_quantity(kind)
}

fn combined_entry(
    left: &ResourceEntry,
    right: &ResourceEntry,
) -> (result: Option<ResourceEntry>)
    requires left.spec_kind() == right.spec_kind(),
    ensures match result {
        Some(entry) => {
            &&& left.spec_quantity().spec_value() as int
                + right.spec_quantity().spec_value() as int <= u64::MAX as int
            &&& entry == ResourceEntry {
                kind: left.spec_kind(),
                quantity: ResourceQuantity(
                    (left.spec_quantity().spec_value() as int
                        + right.spec_quantity().spec_value() as int) as u64,
                ),
            }
        }
        None => left.spec_quantity().spec_value() as int
            + right.spec_quantity().spec_value() as int > u64::MAX as int,
    },
{
    let quantity = left.quantity().get().checked_add(right.quantity().get())?;
    Some(ResourceEntry::new(
        left.kind(),
        ResourceQuantity::from_wire(quantity),
    ))
}

proof fn added_entries_push_both(
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
    left_entry: ResourceEntry,
    right_entry: ResourceEntry,
)
    requires
        left.len() == 0
            || left.last().spec_kind().spec_tag() < left_entry.spec_kind().spec_tag(),
        right.len() == 0
            || right.last().spec_kind().spec_tag() < right_entry.spec_kind().spec_tag(),
        left_entry.spec_kind() == right_entry.spec_kind(),
        left_entry.spec_quantity().spec_value() as int
            + right_entry.spec_quantity().spec_value() as int <= u64::MAX as int,
    ensures
        added_entries(left.push(left_entry), right.push(right_entry))
            == added_entries(left, right).push(ResourceEntry {
                kind: left_entry.spec_kind(),
                quantity: ResourceQuantity(
                    (left_entry.spec_quantity().spec_value() as int
                        + right_entry.spec_quantity().spec_value() as int) as u64,
                ),
            }),
{
    assert(left.push(left_entry).last() == left_entry);
    assert(left.push(left_entry).drop_last() =~= left);
    assert(right.push(right_entry).last() == right_entry);
    assert(right.push(right_entry).drop_last() =~= right);
    assert(left_entry.spec_kind().spec_tag() == right_entry.spec_kind().spec_tag());
    reveal(added_entries);
}

fn merge(
    left_vector: &ResourceVector,
    right_vector: &ResourceVector,
    maximum_dimensions: u16,
) -> (outcome: VectorAddition)
    ensures match outcome {
        VectorAddition::Value(vector) => {
            &&& addition_admissible(
                left_vector.spec_entries(), right_vector.spec_entries(), maximum_dimensions)
            &&& addition_result(&vector, left_vector, right_vector)
            &&& addition_quantities(&vector, left_vector, right_vector)
            &&& vector.spec_entries().len()
                == union_dimensions(left_vector.spec_entries(), right_vector.spec_entries())
        }
        VectorAddition::Overflow =>
            addition_overflows(left_vector.spec_entries(), right_vector.spec_entries()),
        VectorAddition::LimitExceeded => {
            &&& !addition_overflows(left_vector.spec_entries(), right_vector.spec_entries())
            &&& union_dimensions(left_vector.spec_entries(), right_vector.spec_entries())
                > maximum_dimensions as int
        }
    },
{
    match merge::merge_entries(left_vector, right_vector) {
        merge::MergeOutcome::Value(entries) => {
            if entries.len() > usize::from(maximum_dimensions) {
                VectorAddition::LimitExceeded
            } else {
                proof {
                    assert(crate::resource::capacity::entries_canonical(entries@)) by {
                        reveal(crate::resource::capacity::entries_canonical);
                    }
                }
                let vector = ResourceVector(entries);
                proof {
                    assert(addition_result(&vector, left_vector, right_vector));
                    assert(addition_quantities(&vector, left_vector, right_vector)) by {
                        assert forall |kind: ResourceKind| #![auto]
                            vector.spec_quantity(kind)
                                == left_vector.spec_quantity(kind)
                                    + right_vector.spec_quantity(kind) by {
                            vector.quantity_matches_entries(kind);
                        }
                    }
                }
                VectorAddition::Value(vector)
            }
        }
        merge::MergeOutcome::Overflow => VectorAddition::Overflow,
    }
}

pub(super) fn add(
    left_vector: &ResourceVector,
    right_vector: &ResourceVector,
    maximum_dimensions: u16,
) -> (outcome: VectorAddition)
    ensures match outcome {
        VectorAddition::Value(vector) => {
            &&& addition_admissible(
                left_vector.spec_entries(), right_vector.spec_entries(), maximum_dimensions)
            &&& addition_result(&vector, left_vector, right_vector)
            &&& addition_quantities(&vector, left_vector, right_vector)
            &&& vector.spec_entries().len()
                == union_dimensions(left_vector.spec_entries(), right_vector.spec_entries())
        }
        VectorAddition::Overflow =>
            addition_overflows(left_vector.spec_entries(), right_vector.spec_entries()),
        VectorAddition::LimitExceeded => {
            &&& !addition_overflows(left_vector.spec_entries(), right_vector.spec_entries())
            &&& union_dimensions(left_vector.spec_entries(), right_vector.spec_entries())
                > maximum_dimensions as int
        }
    },
{
    merge(left_vector, right_vector, maximum_dimensions)
}

} // verus!
