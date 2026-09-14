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
        let mut index = 0;
        while index < self.observations.len()
            invariant index <= self.observations.len(),
            decreases self.observations.len() - index,
        {
            if self.observations[index].id() == id { return Ok(self.observations[index]); }
            index += 1;
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
