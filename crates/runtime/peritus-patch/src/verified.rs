//! Small executable planning predicates shared with Verus.

#[doc = " Checks the portable byte bound of a nonempty path representation."]
#[must_use]
pub const fn path_bounds_valid(bytes: usize) -> bool {
    bytes > 0 && bytes <= 4_096
}
#[doc = " Checks nonempty operation and aggregate final-byte bounds."]
#[must_use]
pub const fn patch_bounds_valid(
    operations: usize,
    final_bytes: usize,
    operation_limit: usize,
    byte_limit: usize,
) -> bool {
    operations > 0 && operations <= operation_limit && final_bytes <= byte_limit
}
#[doc = " Checks the three scalar workspace bindings accepted by the planner."]
#[must_use]
pub const fn workspace_version_matches(
    workspace_matches: bool,
    expected_generation: u64,
    current_generation: u64,
    expected_revision: u64,
    current_revision: u64,
) -> bool {
    workspace_matches
        && expected_generation == current_generation
        && expected_revision == current_revision
}
#[doc = " Checks all exact regular-file identity dimensions."]
#[must_use]
pub const fn file_identity_matches(
    expected_size: u64,
    observed_size: u64,
    digest_matches: bool,
    expected_mode: u8,
    observed_mode: u8,
) -> bool {
    expected_size == observed_size && digest_matches && expected_mode == observed_mode
}
#[doc = " A target is classifiable only when it is exactly a preimage or a postimage."]
#[must_use]
pub const fn target_is_recoverable(matches_pre: bool, matches_post: bool) -> bool {
    matches_pre || matches_post
}
