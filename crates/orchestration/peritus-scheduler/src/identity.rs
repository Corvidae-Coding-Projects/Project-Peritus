//! Scheduler-owned stable nonzero identities.

mod order;
#[cfg(test)]
mod tests;

use crate::{SchedulerError, SchedulerErrorKind};
#[cfg(verus_only)]
use core::cmp::Ordering;
use peritus_types::ActorId;
use vstd::prelude::*;

verus! {

/// Exact canonical-byte identity for an actor named by scheduler state.
pub open spec fn actor_ids_match(left: ActorId, right: ActorId) -> bool {
    left.spec_bytes() == right.spec_bytes()
}

/// Compares the complete canonical actor identities used by production admission.
pub const fn actor_ids_same(left: ActorId, right: ActorId) -> (result: bool)
    ensures result == actor_ids_match(left, right),
{
    let left_bytes = left.into_bytes();
    let right_bytes = right.into_bytes();
    let mut index = 0;
    while index < 16
        invariant
            index <= 16,
            left_bytes == left.spec_bytes(),
            right_bytes == right.spec_bytes(),
            forall |prior: int| 0 <= prior < index ==>
                left_bytes[prior] == right_bytes[prior],
        decreases 16 - index,
    {
        if left_bytes[index] != right_bytes[index] {
            assert(left_bytes != right_bytes);
            assert(left.spec_bytes() != right.spec_bytes());
            assert(!actor_ids_match(left, right));
            return false;
        }
        index += 1;
    }
    assert(left_bytes@ =~= right_bytes@) by {
        assert forall |at: int| 0 <= at < left_bytes@.len()
            implies left_bytes@[at] == right_bytes@[at] by {
        }
    }
    assert(left_bytes == right_bytes);
    assert(actor_ids_match(left, right));
    true
}

/// Identifies one immutable run-scoped scheduler aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchedulerId([u8; 16]);

} // verus!

impl SchedulerId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

verus! {

/// Identifies one immutable admitted scheduler work item.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkId([u8; 16]);

impl WorkId {
    /// Logical view of every canonical work-identity byte.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0 }

    /// Exact canonical lexicographic order of work identities.
    pub open spec fn spec_precedes(&self, other: &Self) -> bool {
        peritus_types::canonical_byte_order_from(
            self.spec_bytes()@,
            other.spec_bytes()@,
            0,
        ) == Ordering::Less
    }

    /// Returns canonical lexicographic order for deterministic selection.
    pub(crate) const fn precedes(&self, other: &Self) -> (result: bool)
        ensures result == self.spec_precedes(other),
    {
        let mut index = 0;
        while index < 16
            invariant
                index <= 16,
                self.spec_precedes(other) ==
                    (peritus_types::canonical_byte_order_from(
                        self.spec_bytes()@,
                        other.spec_bytes()@,
                        index as nat,
                    ) == Ordering::Less),
            decreases 16 - index,
        {
            if self.0[index] < other.0[index] {
                return true;
            }
            if self.0[index] > other.0[index] {
                return false;
            }
            index += 1;
        }
        false
    }

}

} // verus!

impl WorkId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

verus! {

/// Identifies one registered scheduler worker.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkerId([u8; 16]);

impl WorkerId {
    /// Logical view of every canonical worker-identity byte.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0 }

    /// Exact canonical lexicographic order of worker identities.
    pub open spec fn spec_precedes(&self, other: &Self) -> bool {
        peritus_types::canonical_byte_order_from(
            self.spec_bytes()@,
            other.spec_bytes()@,
            0,
        ) == Ordering::Less
    }

    /// Returns canonical lexicographic order for verified insertion.
    pub(crate) const fn precedes(&self, other: &Self) -> (result: bool)
        ensures result == self.spec_precedes(other),
    {
        let mut index = 0;
        while index < 16
            invariant
                index <= 16,
                self.spec_precedes(other) ==
                    (peritus_types::canonical_byte_order_from(
                        self.spec_bytes()@,
                        other.spec_bytes()@,
                        index as nat,
                    ) == Ordering::Less),
            decreases 16 - index,
        {
            if self.0[index] < other.0[index] {
                return true;
            }
            if self.0[index] > other.0[index] {
                return false;
            }
            index += 1;
        }
        false
    }

}

} // verus!

impl WorkerId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

verus! {

/// Identifies one durable work-attempt reservation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DispatchId([u8; 16]);

impl DispatchId {
    /// Logical view of every canonical dispatch-identity byte.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0 }

    /// Exact canonical lexicographic order of dispatch identities.
    pub open spec fn spec_precedes(&self, other: &Self) -> bool {
        peritus_types::canonical_byte_order_from(
            self.spec_bytes()@,
            other.spec_bytes()@,
            0,
        ) == Ordering::Less
    }

    /// Returns canonical lexicographic ordering used by scheduler-owned sorted vectors.
    pub(crate) const fn precedes(&self, other: &Self) -> (result: bool)
        ensures result == self.spec_precedes(other),
    {
        let mut index = 0;
        while index < 16
            invariant
                index <= 16,
                self.spec_precedes(other) ==
                    (peritus_types::canonical_byte_order_from(
                        self.spec_bytes()@,
                        other.spec_bytes()@,
                        index as nat,
                    ) == Ordering::Less),
            decreases 16 - index,
        {
            if self.0[index] < other.0[index] {
                return true;
            }
            if self.0[index] > other.0[index] {
                return false;
            }
            index += 1;
        }
        false
    }

}

/// Returns exact membership in a canonical dispatch-identity sequence.
pub fn dispatch_id_is_used(values: &[DispatchId], id: DispatchId) -> (result: bool)
    ensures result == values@.contains(id),
{
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values@.len(),
            forall |prior: int| 0 <= prior < index ==> values@[prior] != id,
        decreases values@.len() - index,
    {
        if values[index].same(&id) {
            assert(values@.contains(id));
            return true;
        }
        index += 1;
    }
    assert(!values@.contains(id)) by {
        if values@.contains(id) {
            let at = choose |at: int| 0 <= at < values@.len() && values@[at] == id;
            assert(false);
        }
    }
    false
}

} // verus!

impl DispatchId {
    /// Canonical binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a checked nonzero identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn new(bytes: [u8; 16]) -> Result<Self, SchedulerError> {
        checked_identity(bytes).map(Self)
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

fn checked_identity(bytes: [u8; 16]) -> Result<[u8; 16], SchedulerError> {
    if bytes == [0; 16] {
        Err(crate::error::reject(
            SchedulerErrorKind::InvalidInput,
            "all-zero scheduler identity is reserved",
        ))
    } else {
        Ok(bytes)
    }
}
