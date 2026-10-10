//! Executable and mathematical terminal ordering predicates.

#[doc = " Returns whether an output chunk occupies the exact next sequence and byte offset."]
#[must_use]
pub fn output_is_contiguous(
    next_sequence: u64,
    next_offset: u64,
    sequence: u64,
    offset: u64,
    bytes: usize,
) -> bool {
    if bytes as u128 > u128::from(u64::MAX) {
        return false;
    }
    let length = bytes as u64;
    sequence == next_sequence
        && offset == next_offset
        && bytes > 0
        && length <= u64::MAX - next_offset
}
#[doc = " Returns whether current output accounting is representable and nonterminal."]
#[must_use]
pub const fn output_position_is_valid(next_sequence: u64, terminal: bool) -> bool {
    !terminal && next_sequence < u64::MAX
}
