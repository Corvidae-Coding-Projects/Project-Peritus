//! Executable deterministic rules shared by ordinary Rust and Verus verification.

#[doc = " Adds reserved and total quota bytes with checked arithmetic."]
pub const fn checked_quota_totals(used: u64, reserved: u64, added: u64) -> Option<(u64, u64)> {
    match reserved.checked_add(added) {
        Some(reserved_after) => match used.checked_add(reserved_after) {
            Some(total_after) => Some((reserved_after, total_after)),
            None => None,
        },
        None => None,
    }
}
#[doc = " Returns exactly whether a quarantined object is eligible for a later-generation sweep."]
pub const fn sweep_is_later(quarantined_at: u64, sweep_at: u64) -> bool {
    quarantined_at > 0 && quarantined_at < sweep_at
}
#[doc = " Returns exactly whether caller, expected-size, and configured writer bounds are consistent."]
pub const fn write_bounds_valid(
    expected_size: u64,
    declared_limit: u64,
    configured_limit: u64,
) -> bool {
    declared_limit > 0 && expected_size <= declared_limit && declared_limit <= configured_limit
}
