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

} // verus!

#[cfg(test)]
mod tests {
    use super::traversal_bounds_valid;

    #[test]
    fn independent_bounds_fail_closed() {
        assert!(traversal_bounds_valid(2, 10, 64, 100_000));
        assert!(!traversal_bounds_valid(0, 10, 64, 100_000));
        assert!(!traversal_bounds_valid(2, 0, 64, 100_000));
    }
}
