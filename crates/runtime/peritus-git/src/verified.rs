//! Small executable validation predicates shared with the Verus model.

#[doc = " Returns whether an object-identifier representation has a supported exact length."]
pub const fn supported_object_hex_length(length: usize) -> bool {
    length == 40 || length == 64
}
#[doc = " Returns whether one byte belongs to the safe worktree-name alphabet."]
pub const fn worktree_name_byte_allowed(byte: u8) -> bool {
    (byte >= b'a' && byte <= b'z')
        || (byte >= b'A' && byte <= b'Z')
        || (byte >= b'0' && byte <= b'9')
        || byte == b'-'
        || byte == b'_'
}
#[doc = " Exact three-way Git reconciliation decision over already checked observations."]
#[allow(clippy::fn_params_excessive_bools)]
pub const fn reconciliation_is_clean(
    detached: bool,
    head_matches: bool,
    tree_matches: bool,
    has_worktree_change: bool,
    has_untracked: bool,
    has_ignored: bool,
    allow_ignored: bool,
) -> bool {
    detached
        && head_matches
        && tree_matches
        && !has_worktree_change
        && !has_untracked
        && (allow_ignored || !has_ignored)
}
