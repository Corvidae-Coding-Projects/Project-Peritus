//! Executable and mathematical delivery-safety predicates.

use vstd::prelude::*;

/// Returns whether `after` is the exact non-wrapping successor of `before`.
#[must_use]
pub const fn cursor_is_successor(before: u64, after: u64) -> bool {
    match before.checked_add(1) {
        Some(expected) => after == expected,
        None => false,
    }
}

/// Returns whether a cumulative acknowledgement stays within contiguous delivery.
#[must_use]
pub const fn acknowledgement_is_legal(
    acknowledged: u64,
    delivered: u64,
    candidate: u64,
    gap_active: bool,
) -> bool {
    !gap_active && acknowledged <= candidate && candidate <= delivered
}

/// Returns whether the retained delivery window agrees with cursor accounting.
#[must_use]
pub fn delivery_window_is_safe(
    acknowledged: u64,
    delivered: u64,
    in_flight: usize,
    maximum: usize,
) -> bool {
    acknowledged <= delivered
        && u64::try_from(in_flight).is_ok_and(|count| delivered - acknowledged == count)
        && in_flight <= maximum
}

verus! {

/// Mathematical cursor-successor rule for `INV-024 DeliverySafety`.
pub open spec fn spec_cursor_successor(before: int, after: int) -> bool {
    0 <= before && after == before + 1
}

/// Mathematical cumulative-acknowledgement rule for `INV-024 DeliverySafety`.
pub open spec fn spec_acknowledgement_legal(
    acknowledged: int,
    delivered: int,
    candidate: int,
    gap_active: bool,
) -> bool {
    !gap_active && 0 <= acknowledged && acknowledged <= candidate && candidate <= delivered
}

/// Advancing an acknowledgement through delivered data cannot exceed delivery.
pub proof fn legal_ack_never_exceeds_delivery(
    acknowledged: int,
    delivered: int,
    candidate: int,
)
    requires
        spec_acknowledgement_legal(acknowledged, delivered, candidate, false),
    ensures
        candidate <= delivered,
        acknowledged <= candidate,
{
}

} // verus!
