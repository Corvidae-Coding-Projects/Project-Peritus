//! Exact-view accounting; savings compare with this invocation's unreduced complete exchanges.

use super::super::{
    error,
    memory::LocalMemory,
    record::{
        ViewValidation, WORKING_SELECTION_FRONTIER_SCHEMA_VERSION, WorkingSelectionFrontier,
    },
    view_binding,
};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{WorkingEntryStatus, WorkingSelectionReconciliation};
use peritus_model_protocol::ProviderProfile;

impl LocalMemory {
    pub(super) fn view_validation(
        &self,
        profile: &ProviderProfile,
        estimated: u64,
        uncompacted: u64,
        selected: Vec<u64>,
        omitted_entries: usize,
        reconciliation: Option<&WorkingSelectionReconciliation>,
    ) -> Result<ViewValidation, DeveloperLoopError> {
        let archive_bytes = self.sources.iter().try_fold(0_u64, |total, source| {
            total
                .checked_add(source.artifact.bytes)
                .ok_or_else(|| error("archive accounting overflow"))
        })?;
        let stale_entries = self
            .state
            .entries(self.state.binding())
            .map_err(|_| error("validation scope mismatch"))?
            .iter()
            .filter(|entry| entry.status() == WorkingEntryStatus::Stale)
            .count();
        Ok(ViewValidation {
            model_revision: self.model_revision,
            state_revision: self.state.revision(),
            through_observation: self.state.through_observation(),
            profile: profile.profile_id().into_bytes(),
            profile_revision: profile.revision(),
            estimated_input_tokens: estimated,
            max_input_tokens: profile.limits().max_input_tokens(),
            uncompacted_input_tokens: uncompacted,
            input_tokens_saved: uncompacted.saturating_sub(estimated),
            archive_bytes,
            stale_entries,
            selected_observations: selected,
            omitted_entries,
            pending_operations: self.transcript.pending.len(),
            local_compactor_failures: self.local_compactor_failures,
            retrieval_calls: self.retrieval_calls,
            working_selection_frontier: reconciliation.map(|reconciliation| {
                WorkingSelectionFrontier {
                    schema_version: WORKING_SELECTION_FRONTIER_SCHEMA_VERSION,
                    state_revision: reconciliation.state_revision(),
                    available_tokens: reconciliation.available_tokens(),
                    minimum_required_tokens: reconciliation.minimum_required_tokens(),
                    required_digest: reconciliation.required_digest(),
                    required_entries: reconciliation.required().len(),
                    referenced_entries: reconciliation.referenced().iter()
                        .map(|id| id.into_bytes())
                        .collect(),
                    focus_entry: reconciliation.focus().map(|id| id.into_bytes()),
                }
            }),
            tool_policy: Some(view_binding::tool_policy(&self.tools)?),
            segment_continuation: self.segment_continuation.clone(),
        })
    }
}
