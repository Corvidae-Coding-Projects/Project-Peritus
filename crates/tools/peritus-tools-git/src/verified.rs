//! Executable Git observation-bound refinements verified by Verus.

#[doc = " Checks both diff dimensions against their hard maxima."]
#[must_use]
pub const fn diff_bounds_valid(
    entries: u32,
    bytes: u64,
    maximum_entries: u32,
    maximum_bytes: u64,
) -> bool {
    entries > 0 && entries <= maximum_entries && bytes > 0 && bytes <= maximum_bytes
}
