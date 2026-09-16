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
    /// Logical observation bound.
    pub closed spec fn spec_observations(self) -> nat { self.observations as nat }
    /// Logical retained-entry bound.
    pub closed spec fn spec_entries(self) -> nat { self.entries as nat }
    /// Logical per-entry byte bound.
    pub closed spec fn spec_entry_bytes(self) -> nat { self.entry_bytes as nat }
    /// Logical per-reference-list bound.
    pub closed spec fn spec_links(self) -> nat { self.links as nat }
    /// Logical atomic-operation bound.
    pub closed spec fn spec_operations(self) -> nat { self.operations as nat }

    /// Initial working-state envelope. Exhaustion is an explicit error, never silent forgetting.
    #[must_use]
    pub const fn standard() -> (result: Self)
        ensures
            result.spec_observations() == 65_535,
            result.spec_entries() == 512,
            result.spec_entry_bytes() == 2_048,
            result.spec_links() == 32,
            result.spec_operations() == 32,
    {
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
    ) -> (result: Result<Self, WorkingError>)
        ensures match result {
            Ok(limits) => {
                &&& limits.spec_observations() == observations
                &&& limits.spec_entries() == entries
                &&& limits.spec_entry_bytes() == entry_bytes
                &&& limits.spec_links() == links
                &&& limits.spec_operations() == operations
            }
            Err(_) => true,
        },
    {
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
    pub const fn observations(self) -> (result: usize)
        ensures result as nat == self.spec_observations(),
    { self.observations }
    /// Maximum retained entries, including stale and superseded entries.
    #[must_use]
    pub const fn entries(self) -> (result: usize)
        ensures result as nat == self.spec_entries(),
    { self.entries }
    /// Maximum bytes in one derived entry.
    #[must_use]
    pub const fn entry_bytes(self) -> (result: usize)
        ensures result as nat == self.spec_entry_bytes(),
    { self.entry_bytes }
    /// Maximum elements in each source, dependency, or file-validity list.
    #[must_use]
    pub const fn links(self) -> (result: usize)
        ensures result as nat == self.spec_links(),
    { self.links }
    /// Maximum entry operations in one atomic delta.
    #[must_use]
    pub const fn operations(self) -> (result: usize)
        ensures result as nat == self.spec_operations(),
    { self.operations }
}
}
