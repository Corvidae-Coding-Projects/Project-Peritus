//! Executable decisions verified over observed path lengths, ownership and permissions.

use vstd::prelude::*;

verus! {

/// Selects only addresses whose bytes fit the target socket representation.
pub const fn path_fits(length: usize, maximum: usize) -> (fits: bool)
    ensures fits == (length <= maximum),
{
    length <= maximum
}

/// Requires the exact owner and private permission bits for a real runtime directory.
#[cfg(unix)]
pub const fn private_directory(owner: u32, expected_owner: u32, permissions: u32) -> (valid: bool)
    ensures valid == (owner == expected_owner && permissions == 0o700),
{
    owner == expected_owner && permissions == 0o700
}

/// A shared temporary root must be root-owned and protect entries against other users.
#[cfg(unix)]
pub const fn protected_temporary_root(owner: u32, writable: bool, sticky: bool) -> (valid: bool)
    ensures valid == (owner == 0 && (!writable || sticky)),
{
    owner == 0 && (!writable || sticky)
}

} // verus!
