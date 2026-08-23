//! Executable migration rules shared by ordinary Rust and Verus.

use vstd::prelude::*;

verus! {

/// Returns whether one registry version immediately follows another.
pub const fn versions_are_contiguous(previous: u64, next: u64) -> (result: bool)
    ensures result == (previous < u64::MAX && next == previous + 1),
{
    match previous.checked_add(1) {
        Some(expected) => next == expected,
        None => false,
    }
}

/// Returns whether current and target versions are forward and application-compatible.
pub const fn versions_are_compatible(
    current: u64,
    target: u64,
    minimum: u64,
    maximum: u64,
) -> (result: bool)
    ensures result == (minimum <= current && current <= target && target <= maximum),
{
    minimum <= current && current <= target && target <= maximum
}

/// Combines backup policy monotonically across selected steps.
pub const fn backup_required(previous: bool, step_requires_backup: bool) -> (result: bool)
    ensures result == (previous || step_requires_backup),
{
    previous || step_requires_backup
}

/// Computes checked required capacity: reserve, scratch, and optional whole-database backup.
pub const fn checked_required_space(
    database_bytes: u64,
    scratch_bytes: u64,
    reserve_bytes: u64,
    requires_backup: bool,
) -> (result: Option<u64>)
    ensures
        match result {
            Some(required) => required as int == reserve_bytes as int
                + scratch_bytes as int
                + if requires_backup { database_bytes as int } else { 0 },
            None => reserve_bytes as int
                + scratch_bytes as int
                + if requires_backup { database_bytes as int } else { 0 }
                > u64::MAX as int,
        },
{
    match reserve_bytes.checked_add(scratch_bytes) {
        Some(base) => {
            if requires_backup {
                base.checked_add(database_bytes)
            } else {
                Some(base)
            }
        }
        None => None,
    }
}

} // verus!
