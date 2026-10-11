//! Scheduler-owned stable nonzero identities.

mod order;

use crate::{SchedulerError, SchedulerErrorKind};

use peritus_types::ActorId;

#[doc = " Compares the complete canonical actor identities used by production admission."]
pub const fn actor_ids_same(left: ActorId, right: ActorId) -> bool {
    let left_bytes = left.into_bytes();
    let right_bytes = right.into_bytes();
    let mut index = 0;
    while index < 16 {
        if left_bytes[index] != right_bytes[index] {
            return false;
        }
        index += 1;
    }
    true
}
#[doc = " Identifies one immutable run-scoped scheduler aggregate."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchedulerId([u8; 16]);

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

#[doc = " Identifies one immutable admitted scheduler work item."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkId([u8; 16]);
impl WorkId {
    #[doc = " Returns canonical lexicographic order for deterministic selection."]
    pub(crate) const fn precedes(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < 16 {
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

#[doc = " Identifies one registered scheduler worker."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkerId([u8; 16]);
impl WorkerId {
    #[doc = " Returns canonical lexicographic order for verified insertion."]
    pub(crate) const fn precedes(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < 16 {
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

#[doc = " Identifies one durable work-attempt reservation."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DispatchId([u8; 16]);
impl DispatchId {
    #[doc = " Returns canonical lexicographic ordering used by scheduler-owned sorted vectors."]
    pub(crate) const fn precedes(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < 16 {
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
#[doc = " Returns exact membership in a canonical dispatch-identity sequence."]
pub fn dispatch_id_is_used(values: &[DispatchId], id: DispatchId) -> bool {
    let mut index = 0;
    while index < values.len() {
        if values[index].same(&id) {
            return true;
        }
        index += 1;
    }
    false
}

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
