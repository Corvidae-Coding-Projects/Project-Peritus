//! Shared exact last-stage failure model for active-generation fencing.

pub mod discontinuity;
pub mod expiry;
pub mod holder_loss;
pub mod release;
pub mod revoke;

use vstd::prelude::*;

verus! {

/// Error selected only after a fencing command's specific guards have all passed.
pub(crate) open spec fn final_fence_error(
    aggregate: &crate::LeaseAggregate,
) -> Option<crate::LeaseError> {
    if aggregate.version.spec_value() == u64::MAX as int
        || !aggregate.internal_is_valid()
    {
        Some(crate::LeaseError::CorruptState)
    } else {
        None
    }
}

} // verus!
