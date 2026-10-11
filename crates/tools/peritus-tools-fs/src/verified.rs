//! Executable filesystem bound refinements verified by Verus.

#[doc = " Checks both independent traversal dimensions against hard maxima."]
#[must_use]
pub const fn traversal_bounds_valid(
    depth: u16,
    entries: u32,
    maximum_depth: u16,
    maximum_entries: u32,
) -> bool {
    depth > 0 && depth <= maximum_depth && entries > 0 && entries <= maximum_entries
}
