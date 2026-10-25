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
    compare_from(left, right, 0)
}
