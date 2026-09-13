//! Checked canonical resource dimensions and arithmetic.

use crate::{SchedulerError, SchedulerErrorKind};

/// Stable nonzero resource dimension tag.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceKind(u16);

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

    /// Returns the stable canonical tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        self.0
    }
}

/// Checked positive quantity in a resource dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceQuantity(u64);

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

    /// Returns the exact quantity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) const fn from_wire(value: u64) -> Self {
        Self(value)
    }
}

/// One canonical resource entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceEntry {
    kind: ResourceKind,
    quantity: ResourceQuantity,
}

impl ResourceEntry {
    /// Creates one checked entry.
    #[must_use]
    pub const fn new(kind: ResourceKind, quantity: ResourceQuantity) -> Self {
        Self { kind, quantity }
    }
    /// Returns the dimension.
    #[must_use]
    pub const fn kind(self) -> ResourceKind {
        self.kind
    }
    /// Returns the positive quantity.
    #[must_use]
    pub const fn quantity(self) -> ResourceQuantity {
        self.quantity
    }
}

/// Nonempty unique resource vector in ascending kind order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResourceVector(Vec<ResourceEntry>);

impl ResourceVector {
    /// Creates a canonical vector under a caller-configured dimension bound.
    ///
    /// # Errors
    /// Rejects empty, oversized, duplicated, or unsorted entries.
    pub fn new(
        entries: Vec<ResourceEntry>,
        maximum_dimensions: u16,
    ) -> Result<Self, SchedulerError> {
        if entries.is_empty() || entries.len() > usize::from(maximum_dimensions) {
            return Err(crate::error::reject(
                SchedulerErrorKind::LimitExceeded,
                "resource vector is empty or exceeds its dimension bound",
            ));
        }
        if entries.windows(2).any(|pair| pair[0].kind() >= pair[1].kind()) {
            return Err(crate::error::reject(
                SchedulerErrorKind::NonCanonical,
                "resource entries are duplicated or not in ascending kind order",
            ));
        }
        Ok(Self(entries))
    }

    /// Borrows canonical entries.
    #[must_use]
    pub fn entries(&self) -> &[ResourceEntry] {
        &self.0
    }

    /// Returns a dimension's quantity, with absence representing exact zero usage.
    #[must_use]
    pub fn quantity(&self, kind: ResourceKind) -> u64 {
        self.0
            .binary_search_by_key(&kind, |entry| entry.kind())
            .ok()
            .map_or(0, |index| self.0[index].quantity().get())
    }

    /// Returns whether every requested quantity is provided by `capacity`.
    #[must_use]
    pub fn fits_within(&self, capacity: &Self) -> bool {
        self.0.iter().all(|entry| entry.quantity().get() <= capacity.quantity(entry.kind()))
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
        let mut result = Vec::with_capacity(self.0.len().saturating_add(other.0.len()));
        let (mut left, mut right) = (0, 0);
        while left < self.0.len() || right < other.0.len() {
            match (self.0.get(left), other.0.get(right)) {
                (Some(a), Some(b)) if a.kind() == b.kind() => {
                    let quantity =
                        a.quantity().get().checked_add(b.quantity().get()).ok_or_else(|| {
                            crate::error::reject(
                                SchedulerErrorKind::ResourceConflict,
                                "resource addition overflowed",
                            )
                        })?;
                    result
                        .push(ResourceEntry::new(a.kind(), ResourceQuantity::from_wire(quantity)));
                    left += 1;
                    right += 1;
                }
                (Some(a), Some(b)) if a.kind() < b.kind() => {
                    result.push(*a);
                    left += 1;
                }
                (Some(_) | None, Some(b)) => {
                    result.push(*b);
                    right += 1;
                }
                (Some(a), None) => {
                    result.push(*a);
                    left += 1;
                }
                (None, None) => break,
            }
        }
        Self::new(result, maximum_dimensions)
    }

    /// Subtracts an exact vector, returning `None` for exact zero.
    ///
    /// # Errors
    /// Rejects any absent dimension or underflow.
    pub fn checked_subtract(&self, other: &Self) -> Result<Option<Self>, SchedulerError> {
        let mut result = Vec::with_capacity(self.0.len());
        for entry in &self.0 {
            let subtract = other.quantity(entry.kind());
            let remaining = entry.quantity().get().checked_sub(subtract).ok_or_else(|| {
                crate::error::reject(
                    SchedulerErrorKind::ResourceConflict,
                    "resource subtraction underflowed",
                )
            })?;
            if remaining != 0 {
                result
                    .push(ResourceEntry::new(entry.kind(), ResourceQuantity::from_wire(remaining)));
            }
        }
        if other.0.iter().any(|entry| self.quantity(entry.kind()) == 0) {
            return Err(crate::error::reject(
                SchedulerErrorKind::ResourceConflict,
                "resource subtraction names an absent dimension",
            ));
        }
        Ok((!result.is_empty()).then_some(Self(result)))
    }

    pub(crate) fn validate(&self, maximum_dimensions: u16) -> Result<(), SchedulerError> {
        Self::new(self.0.clone(), maximum_dimensions).map(|_| ())
    }
}
