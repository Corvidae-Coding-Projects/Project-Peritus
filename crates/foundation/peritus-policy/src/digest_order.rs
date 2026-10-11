//! Shared exact ordering for fixed-size correlation digests.

use core::cmp::Ordering;
use peritus_types::Sha256Digest;

const fn compare_digest_from(left: &Sha256Digest, right: &Sha256Digest, index: usize) -> Ordering {
    if index == Sha256Digest::LENGTH {
        Ordering::Equal
    } else if left.as_bytes()[index] < right.as_bytes()[index] {
        Ordering::Less
    } else if left.as_bytes()[index] > right.as_bytes()[index] {
        Ordering::Greater
    } else {
        compare_digest_from(left, right, index + 1)
    }
}
pub const fn compare_digest(left: &Sha256Digest, right: &Sha256Digest) -> Ordering {
    compare_digest_from(left, right, 0)
}
