//! Shared exact ordering for fixed-size correlation digests.

use core::cmp::Ordering;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

const fn compare_digest_from(
    left: &Sha256Digest,
    right: &Sha256Digest,
    index: usize,
) -> (result: Ordering)
    requires index <= Sha256Digest::LENGTH,
    ensures result == peritus_types::canonical_byte_order_from(
        left.spec_bytes()@,
        right.spec_bytes()@,
        index as nat,
    ),
    decreases Sha256Digest::LENGTH - index,
{
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

pub const fn compare_digest(
    left: &Sha256Digest,
    right: &Sha256Digest,
) -> (result: Ordering)
    ensures result == peritus_types::canonical_byte_order_from(
        left.spec_bytes()@,
        right.spec_bytes()@,
        0,
    ),
{
    compare_digest_from(left, right, 0)
}

} // verus!
