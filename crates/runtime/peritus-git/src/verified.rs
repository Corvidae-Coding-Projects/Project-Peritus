//! Small executable validation predicates shared with the Verus model.

use vstd::prelude::*;

verus! {

/// Returns whether an object-identifier representation has a supported exact length.
pub const fn supported_object_hex_length(length: usize) -> (valid: bool)
    ensures valid == (length == 40 || length == 64),
{
    length == 40 || length == 64
}

/// Returns whether one byte belongs to the safe worktree-name alphabet.
pub const fn worktree_name_byte_allowed(byte: u8) -> (allowed: bool)
    ensures allowed == ((byte >= b'a' && byte <= b'z')
        || (byte >= b'A' && byte <= b'Z')
        || (byte >= b'0' && byte <= b'9')
        || byte == b'-'
        || byte == b'_'),
{
    (byte >= b'a' && byte <= b'z')
        || (byte >= b'A' && byte <= b'Z')
        || (byte >= b'0' && byte <= b'9')
        || byte == b'-'
        || byte == b'_'
}

/// Returns whether a bounded status observation can be accepted for parsing.
pub const fn status_shape_within_bounds(
    bytes: usize,
    entries: usize,
    byte_limit: usize,
    entry_limit: usize,
) -> (valid: bool)
    ensures valid == (bytes <= byte_limit && entries <= entry_limit),
{
    bytes <= byte_limit && entries <= entry_limit
}

/// Exact three-way Git reconciliation decision over already checked observations.
#[allow(clippy::fn_params_excessive_bools)] // Independent verified observation facts.
pub const fn reconciliation_is_clean(
    detached: bool,
    head_matches: bool,
    tree_matches: bool,
    has_worktree_change: bool,
    has_untracked: bool,
    has_ignored: bool,
    allow_ignored: bool,
) -> (clean: bool)
    ensures clean == (detached
        && head_matches
        && tree_matches
        && !has_worktree_change
        && !has_untracked
        && (allow_ignored || !has_ignored)),
{
    detached
        && head_matches
        && tree_matches
        && !has_worktree_change
        && !has_untracked
        && (allow_ignored || !has_ignored)
}

} // verus!
