//! Explicit local working-state allocation limits.

use super::WorkingError;

#[doc = " Bounded in-memory source index and working model; archived artifact bytes live in C0."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingLimits {
    observations: usize,
    entries: usize,
    entry_bytes: usize,
    links: usize,
    operations: usize,
}
impl WorkingLimits {
    #[doc = " Default retained-state policy: logical history has no allocation quota."]
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            observations: usize::MAX,
            entries: usize::MAX,
            entry_bytes: usize::MAX,
            links: usize::MAX,
            operations: usize::MAX,
        }
    }
    #[doc = " Creates an explicit caller-selected allocation policy."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects zero bounds. Positive explicit policies remain available to callers."]
    pub const fn new(
        observations: usize,
        entries: usize,
        entry_bytes: usize,
        links: usize,
        operations: usize,
    ) -> Result<Self, WorkingError> {
        if observations == 0 || entries == 0 || entry_bytes == 0 || links == 0 || operations == 0 {
            return Err(WorkingError::InvalidLimit);
        }
        Ok(Self { observations, entries, entry_bytes, links, operations })
    }
    #[doc = " Maximum exact source locators in one lineage."]
    #[must_use]
    pub const fn observations(self) -> usize {
        self.observations
    }
    #[doc = " Maximum retained entries, including stale and superseded entries."]
    #[must_use]
    pub const fn entries(self) -> usize {
        self.entries
    }
    #[doc = " Maximum bytes in one derived entry."]
    #[must_use]
    pub const fn entry_bytes(self) -> usize {
        self.entry_bytes
    }
    #[doc = " Maximum elements in each source, dependency, or file-validity list."]
    #[must_use]
    pub const fn links(self) -> usize {
        self.links
    }
    #[doc = " Maximum entry operations in one atomic delta."]
    #[must_use]
    pub const fn operations(self) -> usize {
        self.operations
    }
}
