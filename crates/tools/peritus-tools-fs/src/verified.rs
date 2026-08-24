//! Executable filesystem bound refinements verified by Verus.

use vstd::prelude::*;

verus! {

/// Mathematical validity of one recursive traversal bound.
pub open spec fn traversal_bounds_valid_spec(
    depth: u16,
    entries: u32,
    maximum_depth: u16,
    maximum_entries: u32,
) -> bool {
    0 < depth <= maximum_depth && 0 < entries <= maximum_entries
}

/// Checks both independent traversal dimensions against hard maxima.
#[must_use]
pub const fn traversal_bounds_valid(
    depth: u16,
    entries: u32,
    maximum_depth: u16,
    maximum_entries: u32,
) -> (result: bool)
    ensures result == traversal_bounds_valid_spec(
        depth,
        entries,
        maximum_depth,
        maximum_entries,
    ),
{
    depth > 0 && depth <= maximum_depth && entries > 0 && entries <= maximum_entries
}

/// Mathematical validity of aggregate search ceilings.
pub open spec fn search_bounds_valid_spec(
    total_bytes: u64,
    matches: u32,
    maximum_bytes: u64,
    maximum_matches: u32,
) -> bool {
    0 < total_bytes <= maximum_bytes && 0 < matches <= maximum_matches
}

/// Checks aggregate search-byte and match ceilings together.
#[must_use]
pub const fn search_bounds_valid(
    total_bytes: u64,
    matches: u32,
    maximum_bytes: u64,
    maximum_matches: u32,
) -> (result: bool)
    ensures result == search_bounds_valid_spec(
        total_bytes,
        matches,
        maximum_bytes,
        maximum_matches,
    ),
{
    total_bytes > 0
        && total_bytes <= maximum_bytes
        && matches > 0
        && matches <= maximum_matches
}

} // verus!

#[cfg(test)]
mod tests {
    use super::{search_bounds_valid, traversal_bounds_valid};

    #[test]
    fn independent_bounds_fail_closed() {
        assert!(traversal_bounds_valid(2, 10, 64, 100_000));
        assert!(!traversal_bounds_valid(0, 10, 64, 100_000));
        assert!(!traversal_bounds_valid(2, 0, 64, 100_000));
        assert!(search_bounds_valid(1, 1, 1024, 100));
        assert!(!search_bounds_valid(1025, 1, 1024, 100));
    }
}
