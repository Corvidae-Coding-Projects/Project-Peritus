//! Bound reads from immutable working state.

use super::validation::find_entry;
use super::{
    ObservationId, ObservationSource, WorkingBinding, WorkingEntry, WorkingError, WorkingState,
};
use crate::ContextNodeId;

impl WorkingState {
    #[doc = " Reads host-owned literal requirement references and unresolved operation state."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects another lineage or stale conversation before exposing any source handles."]
    pub fn protocol(
        &self,
        binding: WorkingBinding,
    ) -> Result<&super::WorkingProtocol, WorkingError> {
        self.check_binding(binding)?;
        Ok(&self.protocol)
    }
    #[doc = " Reads the retained model only through an exact task/role/conversation binding."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects cross-run, cross-workspace, cross-task, cross-role, and stale-conversation reads."]
    pub fn entries(&self, binding: WorkingBinding) -> Result<&[WorkingEntry], WorkingError> {
        self.check_binding(binding)?;
        Ok(self.entries.as_slice())
    }
    #[doc = " Resolves an exact observation handle without returning or materializing artifact bytes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects mismatched scope or missing handles before the host performs artifact I/O."]
    pub fn observation(
        &self,
        binding: WorkingBinding,
        id: ObservationId,
    ) -> Result<ObservationSource, WorkingError> {
        self.check_binding(binding)?;
        let mut index = 0;
        while index < self.observations.len() {
            if self.observations[index].id() == id {
                return Ok(self.observations[index]);
            }
            index += 1;
        }
        Err(WorkingError::MissingSource)
    }
    #[doc = " Resolves an entry without dropping its stale/superseded status or counterevidence."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects another scope or an unknown entry."]
    #[allow(clippy::option_if_let_else, reason = "Verus does not support Option::map_or")]
    pub fn entry(
        &self,
        binding: WorkingBinding,
        id: ContextNodeId,
    ) -> Result<&WorkingEntry, WorkingError> {
        self.check_binding(binding)?;
        match find_entry(&self.entries, id) {
            Some(index) => Ok(&self.entries[index]),
            None => Err(WorkingError::MissingEntry),
        }
    }
}
