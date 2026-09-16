//! Executable and mathematical terminal ordering predicates.

use vstd::prelude::*;

verus! {

/// Returns whether an output chunk occupies the exact next sequence and byte offset.
#[must_use]
pub fn output_is_contiguous(
    next_sequence: u64,
    next_offset: u64,
    sequence: u64,
    offset: u64,
    bytes: usize,
) -> (contiguous: bool)
    ensures contiguous == (
        sequence == next_sequence && offset == next_offset && bytes > 0
            && next_offset as int + bytes as int <= u64::MAX as int
    ),
{
    if bytes as u128 > u128::from(u64::MAX) {
        return false;
    }
    let length = bytes as u64;
    sequence == next_sequence
        && offset == next_offset
        && bytes > 0
        && length <= u64::MAX - next_offset
}

/// Returns whether current output accounting is representable and nonterminal.
#[must_use]
pub const fn output_position_is_valid(next_sequence: u64, terminal: bool) -> (valid: bool)
    ensures valid == (!terminal && next_sequence < u64::MAX),
{
    !terminal && next_sequence < u64::MAX
}

/// Mathematical output step for `INV-026 TerminalOrdering`.
pub open spec fn spec_output_contiguous(
    expected_sequence: int,
    expected_offset: int,
    sequence: int,
    offset: int,
    length: int,
    next_sequence: int,
    next_offset: int,
) -> bool {
    0 <= expected_sequence && 0 <= expected_offset
        && sequence == expected_sequence && offset == expected_offset && 0 < length
        && next_sequence == sequence + 1 && next_offset == offset + length
}

/// A legal output step advances both fences exactly.
pub proof fn legal_output_advances_exactly(
    expected_sequence: int,
    expected_offset: int,
    sequence: int,
    offset: int,
    length: int,
    next_sequence: int,
    next_offset: int,
)
    requires
        spec_output_contiguous(
            expected_sequence,
            expected_offset,
            sequence,
            offset,
            length,
            next_sequence,
            next_offset,
        ),
    ensures
        next_sequence == expected_sequence + 1,
        next_offset == expected_offset + length,
{
}

/// Terminal state forbids any further output admission.
pub open spec fn spec_terminal_excludes_output(terminal: bool, admitted: bool) -> bool {
    !terminal || !admitted
}

} // verus!
