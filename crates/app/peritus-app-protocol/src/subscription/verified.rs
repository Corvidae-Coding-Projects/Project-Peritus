//! Executable and mathematical delivery-safety predicates.

use vstd::prelude::*;

/// Returns whether `after` strictly advances `before` without assigning meaning to skipped values.
#[must_use]
pub const fn cursor_advances(before: u64, after: u64) -> bool {
    before < after
}

/// Returns whether a cumulative acknowledgement stays within contiguous delivery.
#[must_use]
pub const fn acknowledgement_is_legal(
    acknowledged: u64,
    delivered: u64,
    candidate: u64,
    gap_active: bool,
    delivered_member: bool,
) -> bool {
    !gap_active
        && acknowledged <= candidate
        && candidate <= delivered
        && (candidate == acknowledged || delivered_member)
}

/// Returns whether the retained delivery window agrees with cursor accounting.
#[must_use]
pub const fn delivery_window_is_safe(
    acknowledged: u64,
    delivered: u64,
    in_flight: usize,
    maximum: usize,
) -> bool {
    acknowledged <= delivered && in_flight <= maximum
}

verus! {

/// Mathematical strictly-advancing source-cursor rule for `INV-024 DeliverySafety`.
pub open spec fn spec_cursor_advances(before: int, after: int) -> bool {
    0 <= before && before < after
}

/// Mathematical cumulative-acknowledgement rule for `INV-024 DeliverySafety`.
pub open spec fn spec_acknowledgement_legal(
    acknowledged: int,
    delivered: int,
    candidate: int,
    gap_active: bool,
    delivered_member: bool,
) -> bool {
    !gap_active
        && 0 <= acknowledged
        && acknowledged <= candidate
        && candidate <= delivered
        && (candidate == acknowledged || delivered_member)
}

/// Advancing an acknowledgement through delivered data cannot exceed delivery.
pub proof fn legal_ack_never_exceeds_delivery(
    acknowledged: int,
    delivered: int,
    candidate: int,
)
    requires
        spec_acknowledgement_legal(acknowledged, delivered, candidate, false, true),
    ensures
        candidate <= delivered,
        acknowledged <= candidate,
{
}

} // verus!
