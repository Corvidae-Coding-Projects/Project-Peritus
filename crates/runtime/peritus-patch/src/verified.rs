//! Small executable planning predicates shared with Verus.

use vstd::prelude::*;

verus! {

/// Checks the complete byte/component bounds of a nonempty path representation.
#[must_use]
pub const fn path_bounds_valid(bytes: usize, components: usize) -> (valid: bool)
    ensures valid == (
        bytes > 0 && bytes <= 4_096 && components > 0 && components <= 256
    )
{
    bytes > 0 && bytes <= 4_096 && components > 0 && components <= 256
}

/// Checks nonempty operation and aggregate final-byte bounds.
#[must_use]
pub const fn patch_bounds_valid(
    operations: usize,
    final_bytes: usize,
    operation_limit: usize,
    byte_limit: usize,
) -> (valid: bool)
    ensures valid == (
        operations > 0
            && operations <= operation_limit
            && final_bytes <= byte_limit
    )
{
    operations > 0 && operations <= operation_limit && final_bytes <= byte_limit
}

/// Checks the three scalar workspace bindings accepted by the planner.
#[must_use]
pub const fn workspace_version_matches(
    workspace_matches: bool,
    expected_generation: u64,
    current_generation: u64,
    expected_revision: u64,
    current_revision: u64,
) -> (valid: bool)
    ensures valid == (
        workspace_matches
            && expected_generation == current_generation
            && expected_revision == current_revision
    )
{
    workspace_matches
        && expected_generation == current_generation
        && expected_revision == current_revision
}

/// Checks all exact regular-file identity dimensions.
#[must_use]
pub const fn file_identity_matches(
    expected_size: u64,
    observed_size: u64,
    digest_matches: bool,
    expected_mode: u8,
    observed_mode: u8,
) -> (valid: bool)
    ensures valid == (
        expected_size == observed_size
            && digest_matches
            && expected_mode == observed_mode
    )
{
    expected_size == observed_size && digest_matches && expected_mode == observed_mode
}

/// A target is classifiable only when it is exactly a preimage or a postimage.
#[must_use]
pub const fn target_is_recoverable(matches_pre: bool, matches_post: bool) -> (valid: bool)
    ensures valid == (matches_pre || matches_post)
{
    matches_pre || matches_post
}

} // verus!
