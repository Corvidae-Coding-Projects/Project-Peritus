//! Bound reads from immutable working state.

use super::validation::find_entry;
use super::{ObservationId, ObservationSource, WorkingBinding, WorkingEntry, WorkingError, WorkingState};
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
impl WorkingState {
    /// Reads host-owned literal requirement references and unresolved operation state.
    ///
    /// # Errors
    /// Rejects another lineage or stale conversation before exposing any source handles.
    pub fn protocol(&self, binding: WorkingBinding) -> Result<&super::WorkingProtocol, WorkingError> {
        self.check_binding(binding)?;
        Ok(&self.protocol)
    }
    /// Reads the retained model only through an exact task/role/conversation binding.
    ///
    /// # Errors
    /// Rejects cross-run, cross-workspace, cross-task, cross-role, and stale-conversation reads.
    pub fn entries(&self, binding: WorkingBinding) -> Result<&[WorkingEntry], WorkingError> {
        self.check_binding(binding)?;
        Ok(self.entries.as_slice())
    }

    /// Resolves an exact observation handle without returning or materializing artifact bytes.
    ///
    /// # Errors
    /// Rejects mismatched scope or missing handles before the host performs artifact I/O.
    pub fn observation(
        &self,
        binding: WorkingBinding,
        id: ObservationId,
    ) -> (result: Result<ObservationSource, WorkingError>)
        ensures match result {
            Ok(_) => true,
            Err(error) => error == WorkingError::BindingMismatch
                || error == WorkingError::MissingSource,
        },
    {
        self.check_binding(binding)?;
        // Ingestion and recovery require a contiguous one-based archive. Check
        // the returned identity as well, so this index never accepts a gap or
        // conflicting locator even if an invalid state reaches the accessor.
        let sequence = id.get();
        if sequence == 0 || sequence - 1 > usize::MAX as u64 {
            return Err(WorkingError::MissingSource);
        }
        let index = (sequence - 1) as usize;
        if index < self.observations.len() && self.observations[index].id() == id {
            return Ok(self.observations[index]);
        }
        Err(WorkingError::MissingSource)
    }

    /// Resolves an entry without dropping its stale/superseded status or counterevidence.
    ///
    /// # Errors
    /// Rejects another scope or an unknown entry.
    #[allow(clippy::option_if_let_else, reason = "Verus does not support Option::map_or")]
    pub fn entry(&self, binding: WorkingBinding, id: ContextNodeId) -> Result<&WorkingEntry, WorkingError> {
        self.check_binding(binding)?;
        match find_entry(&self.entries, id) {
            Some(index) => Ok(&self.entries[index]),
            None => Err(WorkingError::MissingEntry),
        }
    }
}
}

#[cfg(not(verus_only))]
pub(super) fn active_projection(entries: &[WorkingEntry]) -> Vec<&WorkingEntry> {
    let mut included = entries
        .iter()
        .filter(|entry| !entry.status().is_retired())
        .map(WorkingEntry::id)
        .collect::<std::collections::BTreeSet<_>>();
    let mut frontier = included.iter().copied().collect::<Vec<_>>();
    let mut cursor = 0;
    while cursor < frontier.len() {
        let id = frontier[cursor];
        cursor += 1;
        let Ok(index) = entries.binary_search_by_key(&id, WorkingEntry::id) else {
            continue;
        };
        for dependency in entries[index].links().depends_on() {
            if included.insert(*dependency) {
                frontier.push(*dependency);
            }
        }
    }
    entries.iter().filter(|entry| included.contains(&entry.id())).collect()
}

#[cfg(not(verus_only))]
impl WorkingState {
    /// Returns the current prompt and update projection with complete dependency closure.
    ///
    /// Stale and superseded records retire from this projection unless a current entry still
    /// depends on them. [`WorkingState::entry`] continues to resolve every retained historical
    /// identifier exactly.
    ///
    /// # Errors
    /// Rejects another lineage or stale conversation before exposing the projection.
    pub fn active_entries(
        &self,
        binding: WorkingBinding,
    ) -> Result<Vec<&WorkingEntry>, WorkingError> {
        self.check_binding(binding)?;
        Ok(active_projection(&self.entries))
    }
}
