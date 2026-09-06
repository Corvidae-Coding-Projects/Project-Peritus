//! Explicit local working-state allocation limits.

use super::WorkingError;
use vstd::prelude::*;

verus! {
/// Bounded in-memory source index and working model; archived artifact bytes live in C0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingLimits {
    observations: usize,
    entries: usize,
    entry_bytes: usize,
    links: usize,
    operations: usize,
}

impl WorkingLimits {
    /// Initial working-state envelope. Exhaustion is an explicit error, never silent forgetting.
    #[must_use]
    pub const fn standard() -> Self {
        Self { observations: 65_535, entries: 512, entry_bytes: 2_048, links: 32, operations: 32 }
    }

    /// Creates limits no wider than the hard implementation envelope.
    ///
    /// # Errors
    /// Rejects zero bounds and allocations wider than the default envelope.
    pub const fn new(
        observations: usize,
        entries: usize,
        entry_bytes: usize,
        links: usize,
        operations: usize,
    ) -> Result<Self, WorkingError> {
        if observations == 0 || observations > 65_535
            || entries == 0 || entries > 512
            || entry_bytes == 0 || entry_bytes > 2_048
            || links == 0 || links > 32
            || operations == 0 || operations > 32
        {
            return Err(WorkingError::InvalidLimit);
        }
        Ok(Self { observations, entries, entry_bytes, links, operations })
    }

    /// Maximum exact source locators in one lineage.
    #[must_use]
    pub const fn observations(self) -> usize { self.observations }
    /// Maximum retained entries, including stale and superseded entries.
    #[must_use]
    pub const fn entries(self) -> usize { self.entries }
    /// Maximum bytes in one derived entry.
    #[must_use]
    pub const fn entry_bytes(self) -> usize { self.entry_bytes }
    /// Maximum elements in each source, dependency, or file-validity list.
    #[must_use]
    pub const fn links(self) -> usize { self.links }
    /// Maximum entry operations in one atomic delta.
    #[must_use]
    pub const fn operations(self) -> usize { self.operations }
}
}
