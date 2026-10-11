//! Exact executable resource quantity and capacity correspondence.

use super::{ResourceEntry, ResourceKind, ResourceVector, VectorAdmission};
use core::cmp::Ordering;

#[doc = " Checks the same adjacent canonical order used by resource-vector admission."]
pub(super) fn order_is_canonical(entries: &[ResourceEntry]) -> bool {
    let mut index = 1;
    while index < entries.len() {
        if entries[index - 1].kind().tag() >= entries[index].kind().tag() {
            return false;
        }
        index += 1;
    }
    true
}
#[doc = " Performs the exact production admission decision before public error construction."]
pub(super) fn admit_entries(
    entries: Vec<ResourceEntry>,
    maximum_dimensions: u16,
) -> VectorAdmission {
    if entries.is_empty() || entries.len() > usize::from(maximum_dimensions) {
        VectorAdmission::LimitExceeded
    } else if !order_is_canonical(entries.as_slice()) {
        VectorAdmission::NonCanonical
    } else {
        VectorAdmission::Accepted(ResourceVector(entries))
    }
}
impl ResourceVector {}
fn indexed_quantity(entries: &[ResourceEntry], kind: ResourceKind) -> u64 {
    let mut low = 0usize;
    let mut high = entries.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let observed = entries[middle].kind().tag();
        let target = kind.tag();
        match observed.cmp(&target) {
            Ordering::Equal => {
                return entries[middle].quantity().get();
            }
            Ordering::Less => {
                low = middle + 1;
            }
            Ordering::Greater => {
                high = middle;
            }
        }
    }
    0
}
fn fits_from(entries: &[ResourceEntry], index: usize, capacity: &ResourceVector) -> bool {
    if index == entries.len() {
        true
    } else {
        let entry = entries[index];
        entry.quantity().get() <= capacity.quantity(entry.kind())
            && fits_from(entries, index + 1, capacity)
    }
}
impl ResourceVector {
    #[doc = " Returns a dimension\'s quantity, with absence representing exact zero usage."]
    #[must_use]
    pub fn quantity(&self, kind: ResourceKind) -> u64 {
        indexed_quantity(&self.0, kind)
    }
    #[doc = " Returns whether every requested quantity is provided by `capacity`."]
    #[must_use]
    pub fn fits_within(&self, capacity: &Self) -> bool {
        let result = fits_from(&self.0, 0, capacity);
        result
    }
}
