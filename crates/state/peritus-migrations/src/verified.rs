//! Executable migration rules shared by ordinary Rust and Verus.

#[doc = " Returns whether one registry version immediately follows another."]
pub const fn versions_are_contiguous(previous: u64, next: u64) -> bool {
    match previous.checked_add(1) {
        Some(expected) => next == expected,
        None => false,
    }
}
#[doc = " Returns whether current and target versions are forward and application-compatible."]
pub const fn versions_are_compatible(
    current: u64,
    target: u64,
    minimum: u64,
    maximum: u64,
) -> bool {
    minimum <= current && current <= target && target <= maximum
}
#[doc = " Combines backup policy monotonically across selected steps."]
pub const fn backup_required(previous: bool, step_requires_backup: bool) -> bool {
    previous || step_requires_backup
}
#[doc = " Computes checked required capacity: reserve, scratch, and optional whole-database backup."]
pub const fn checked_required_space(
    database_bytes: u64,
    scratch_bytes: u64,
    reserve_bytes: u64,
    requires_backup: bool,
) -> Option<u64> {
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
