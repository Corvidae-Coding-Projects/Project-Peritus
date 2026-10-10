//! Verified canonical resource subtraction without output revalidation.

use super::{ResourceEntry, ResourceQuantity, ResourceVector, VectorSubtraction};

enum EntrySubtraction {
    Value(Vec<ResourceEntry>),
    Underflow,
}
fn subtract_entries(left: &ResourceVector, other: &ResourceVector) -> EntrySubtraction {
    let mut entries = Vec::with_capacity(left.0.len());
    let mut index = 0usize;
    while index < left.0.len() {
        let entry = left.0[index];
        let subtract = other.quantity(entry.kind());
        if entry.quantity().get() < subtract {
            return EntrySubtraction::Underflow;
        }
        let remaining = entry.quantity().get() - subtract;
        if remaining != 0 {
            let next = ResourceEntry::new(entry.kind(), ResourceQuantity::from_wire(remaining));
            entries.push(next);
        }
        index += 1;
    }
    EntrySubtraction::Value(entries)
}
fn has_zero_sentinel(left: &ResourceVector, other: &ResourceVector) -> bool {
    let mut other_index = 0usize;
    while other_index < other.0.len() {
        if left.quantity(other.0[other_index].kind()) == 0 {
            return true;
        }
        other_index += 1;
    }
    false
}
pub(super) fn subtract(left: &ResourceVector, other: &ResourceVector) -> VectorSubtraction {
    let entries = match subtract_entries(left, other) {
        EntrySubtraction::Underflow => return VectorSubtraction::Underflow,
        EntrySubtraction::Value(entries) => entries,
    };
    if has_zero_sentinel(left, other) {
        return VectorSubtraction::AbsentDimension;
    }
    if !entries.is_empty() {
        let vector = ResourceVector(entries);
        return VectorSubtraction::Value(vector);
    }
    VectorSubtraction::ExactZero
}
