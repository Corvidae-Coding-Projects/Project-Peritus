//! Verified canonical resource addition used by public arithmetic and capacity aggregation.

use super::{ResourceEntry, ResourceQuantity, ResourceVector, VectorAddition};

mod merge;
mod proofs;

fn combined_entry(left: &ResourceEntry, right: &ResourceEntry) -> Option<ResourceEntry> {
    let quantity = left.quantity().get().checked_add(right.quantity().get())?;
    Some(ResourceEntry::new(left.kind(), ResourceQuantity::from_wire(quantity)))
}
fn merge(
    left_vector: &ResourceVector,
    right_vector: &ResourceVector,
    maximum_dimensions: u16,
) -> VectorAddition {
    match merge::merge_entries(left_vector, right_vector) {
        merge::MergeOutcome::Value(entries) => {
            if entries.len() > usize::from(maximum_dimensions) {
                VectorAddition::LimitExceeded
            } else {
                let vector = ResourceVector(entries);
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
) -> VectorAddition {
    merge(left_vector, right_vector, maximum_dimensions)
}
