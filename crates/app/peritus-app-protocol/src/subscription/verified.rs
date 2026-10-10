//! Executable and mathematical delivery-safety predicates.

#[doc = " Returns whether `after` strictly advances `before` without assigning meaning to skipped values."]
#[must_use]
pub const fn cursor_advances(before: u64, after: u64) -> bool {
    before < after
}
#[doc = " Returns whether a cumulative acknowledgement stays within contiguous delivery."]
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
#[doc = " Returns whether the retained delivery window agrees with cursor accounting."]
#[must_use]
pub const fn delivery_window_is_safe(
    acknowledged: u64,
    delivered: u64,
    in_flight: usize,
    maximum: usize,
) -> bool {
    acknowledged <= delivered && in_flight <= maximum
}
