//! Exact lexicographic ordering of canonical identity bytes.

use core::cmp::Ordering;

fn compare_from(left: &[u8], right: &[u8], index: usize) -> Ordering {
    if index == left.len() {
        if index == right.len() { Ordering::Equal } else { Ordering::Less }
    } else if index == right.len() {
        Ordering::Greater
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_from(left, right, index + 1)
    }
}
#[doc = " Compares complete identities without depending on unrefined derived ordering methods."]
pub fn compare(left: &[u8], right: &[u8]) -> Ordering {
    let result = compare_from(left, right, 0);
    result
}
#[doc = " Preserves the constructor\'s duplicate-before-descending error classification."]
pub fn require_ascending(
    left: &[u8],
    right: &[u8],
    collection: crate::CanonicalEvidenceCollection,
    index: usize,
) -> Result<(), crate::EvidenceError> {
    match compare(left, right) {
        Ordering::Equal => Err(crate::EvidenceError::new(
            crate::EvidenceErrorKind::DuplicateObservation,
            collection,
            index,
        )),
        Ordering::Greater => Err(crate::EvidenceError::new(
            crate::EvidenceErrorKind::NonCanonicalOrder,
            collection,
            index,
        )),
        Ordering::Less => Ok(()),
    }
}
