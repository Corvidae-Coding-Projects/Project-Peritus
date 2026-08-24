//! Executable Git observation-bound refinements verified by Verus.

use vstd::prelude::*;

verus! {

/// Mathematical validity of both independently selected diff bounds.
pub open spec fn diff_bounds_valid_spec(
    entries: u32,
    bytes: u64,
    maximum_entries: u32,
    maximum_bytes: u64,
) -> bool {
    0 < entries <= maximum_entries && 0 < bytes <= maximum_bytes
}

/// Checks both diff dimensions against their hard maxima.
#[must_use]
pub const fn diff_bounds_valid(
    entries: u32,
    bytes: u64,
    maximum_entries: u32,
    maximum_bytes: u64,
) -> (result: bool)
    ensures result == diff_bounds_valid_spec(entries, bytes, maximum_entries, maximum_bytes),
{
    entries > 0 && entries <= maximum_entries && bytes > 0 && bytes <= maximum_bytes
}

} // verus!

#[cfg(test)]
mod tests {
    #[test]
    fn diff_dimensions_are_independent() {
        assert!(super::diff_bounds_valid(10, 1024, 100, 4096));
        assert!(!super::diff_bounds_valid(0, 1024, 100, 4096));
        assert!(!super::diff_bounds_valid(10, 4097, 100, 4096));
    }
}
