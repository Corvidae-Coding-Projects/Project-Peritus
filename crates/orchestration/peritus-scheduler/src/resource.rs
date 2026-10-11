//! Checked canonical resource dimensions and arithmetic.

use crate::{SchedulerError, SchedulerErrorKind};

mod addition;
pub mod capacity;
mod subtraction;

#[doc = " Stable nonzero resource dimension tag."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceKind(u16);
impl ResourceKind {
    #[doc = " Returns the stable canonical tag."]
    #[must_use]
    pub const fn tag(self) -> u16 {
        self.0
    }
}

impl ResourceKind {
    /// CPU execution slots.
    pub const CPU: Self = Self(1);
    /// Resident-memory bytes.
    pub const MEMORY_BYTES: Self = Self(2);
    /// GPU execution slots.
    pub const GPU: Self = Self(3);
    /// Child-process slots.
    pub const PROCESS: Self = Self(4);
    /// Network-operation slots.
    pub const NETWORK: Self = Self(5);

    /// Creates a stable nonzero resource tag.
    ///
    /// # Errors
    /// Rejects zero, which is reserved for protocol evolution.
    pub fn new(tag: u16) -> Result<Self, SchedulerError> {
        if tag == 0 {
            Err(crate::error::reject(SchedulerErrorKind::InvalidInput, "resource-kind tag is zero"))
        } else {
            Ok(Self(tag))
        }
    }
}

#[doc = " Checked positive quantity in a resource dimension."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceQuantity(u64);
impl ResourceQuantity {
    #[doc = " Returns the exact quantity."]
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
    pub(crate) const fn from_wire(value: u64) -> Self {
        Self(value)
    }
}

impl ResourceQuantity {
    /// Creates a positive resource quantity.
    ///
    /// # Errors
    /// Rejects zero.
    pub fn new(value: u64) -> Result<Self, SchedulerError> {
        if value == 0 {
            Err(crate::error::reject(SchedulerErrorKind::InvalidInput, "resource quantity is zero"))
        } else {
            Ok(Self(value))
        }
    }
}

#[doc = " One canonical resource entry."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceEntry {
    kind: ResourceKind,
    quantity: ResourceQuantity,
}
impl ResourceEntry {
    #[doc = " Returns the dimension."]
    #[must_use]
    pub const fn kind(self) -> ResourceKind {
        self.kind
    }
    #[doc = " Returns the positive quantity."]
    #[must_use]
    pub const fn quantity(self) -> ResourceQuantity {
        self.quantity
    }
    #[doc = " Creates one checked entry."]
    #[must_use]
    pub const fn new(kind: ResourceKind, quantity: ResourceQuantity) -> Self {
        Self { kind, quantity }
    }
}

#[doc = " Nonempty unique resource vector in ascending kind order."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct ResourceVector(Vec<ResourceEntry>);
enum VectorAdmission {
    Accepted(ResourceVector),
    LimitExceeded,
    NonCanonical,
}
enum VectorAddition {
    Value(ResourceVector),
    Overflow,
    LimitExceeded,
}
enum VectorSubtraction {
    Value(ResourceVector),
    ExactZero,
    Underflow,
    AbsentDimension,
}
impl ResourceVector {
    #[doc = " Borrows canonical entries."]
    #[must_use]
    pub fn entries(&self) -> &[ResourceEntry] {
        &self.0
    }
    #[doc = " Adds two canonical vectors for internal aggregate accounting."]
    pub(crate) fn aggregate_add(&self, other: &Self) -> Option<Self> {
        match addition::add(self, other, u16::MAX) {
            VectorAddition::Value(vector) => Some(vector),
            VectorAddition::Overflow | VectorAddition::LimitExceeded => None,
        }
    }
}
impl Clone for ResourceVector {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl ResourceVector {
    /// Creates a canonical vector under a caller-configured dimension bound.
    ///
    /// # Errors
    /// Rejects empty, oversized, duplicated, or unsorted entries.
    pub fn new(
        entries: Vec<ResourceEntry>,
        maximum_dimensions: u16,
    ) -> Result<Self, SchedulerError> {
        match capacity::admit_entries(entries, maximum_dimensions) {
            VectorAdmission::Accepted(vector) => Ok(vector),
            VectorAdmission::LimitExceeded => Err(crate::error::reject(
                SchedulerErrorKind::LimitExceeded,
                "resource vector is empty or exceeds its dimension bound",
            )),
            VectorAdmission::NonCanonical => Err(crate::error::reject(
                SchedulerErrorKind::NonCanonical,
                "resource entries are duplicated or not in ascending kind order",
            )),
        }
    }

    /// Adds two vectors without wrapping or losing canonical ordering.
    ///
    /// # Errors
    /// Rejects arithmetic overflow or a result above the dimension bound.
    pub fn checked_add(
        &self,
        other: &Self,
        maximum_dimensions: u16,
    ) -> Result<Self, SchedulerError> {
        match addition::add(self, other, maximum_dimensions) {
            VectorAddition::Value(vector) => Ok(vector),
            VectorAddition::Overflow => Err(crate::error::reject(
                SchedulerErrorKind::ResourceConflict,
                "resource addition overflowed",
            )),
            VectorAddition::LimitExceeded => Err(crate::error::reject(
                SchedulerErrorKind::LimitExceeded,
                "resource vector is empty or exceeds its dimension bound",
            )),
        }
    }

    /// Subtracts an exact vector, returning `None` for exact zero.
    ///
    /// # Errors
    /// Rejects any absent dimension or underflow.
    pub fn checked_subtract(&self, other: &Self) -> Result<Option<Self>, SchedulerError> {
        match subtraction::subtract(self, other) {
            VectorSubtraction::Value(vector) => Ok(Some(vector)),
            VectorSubtraction::ExactZero => Ok(None),
            VectorSubtraction::Underflow => Err(crate::error::reject(
                SchedulerErrorKind::ResourceConflict,
                "resource subtraction underflowed",
            )),
            VectorSubtraction::AbsentDimension => Err(crate::error::reject(
                SchedulerErrorKind::ResourceConflict,
                "resource subtraction names an absent dimension",
            )),
        }
    }

    pub(crate) fn validate(&self, maximum_dimensions: u16) -> Result<(), SchedulerError> {
        Self::new(self.0.clone(), maximum_dimensions).map(|_| ())
    }
}
