//! Read-only conversation projection and daemon-owned source access.

use super::LiveConversation;
#[cfg(not(verus_only))]
use peritus_agent::DeveloperInteraction;
use peritus_product_runner::{
    ContextSourcePage, ContextSourceSlice, ConversationView, WorkspaceMutationKind,
    control::{HostPermissions, WorkspaceMutationBaseline},
};
use peritus_review::{
    ProductFinding, ProductFindingBodyPublisher, ProductFindingBodyReference,
    ProductReviewError, ProductReviewSummaryReference,
};
use std::path::PathBuf;

impl ConversationView for LiveConversation {
    fn uses_explicit_media(&self) -> bool {
        true
    }
    fn stable_request_context(&self) -> String {
        self.projected_governing_state().stable_context
    }
    fn reference_authority_context(&self) -> String {
        self.projected_governing_state().reference_authority
    }
    fn context_sources(&self, after: Option<u64>) -> Result<ContextSourcePage, String> {
        self.service
            .combined_context_sources(self.run_id, after)
            .map_err(|error| error.describe())
    }
    fn read_context_source(
        &self,
        source: u64,
        offset: u64,
    ) -> Result<ContextSourceSlice, String> {
        self.service
            .read_combined_context_source(self.run_id, source, offset)
            .map_err(|error| error.describe())
    }
    fn finding_body_publisher(&self) -> Option<&dyn ProductFindingBodyPublisher> {
        Some(self)
    }
    fn adopt_finding_state(&self, finding_state: &str) -> Result<(), String> {
        self.service
            .adopt_finding_state(self.run_id, finding_state)
            .map_err(|error| error.describe())
    }
    fn request_sources(&self, after: Option<u64>) -> Result<ContextSourcePage, String> {
        self.request_source_snapshot()
            .and_then(|snapshot| snapshot.request_sources(after).map_err(Into::into))
            .map_err(|error| error.describe())
    }
    fn request_sources_required(&self) -> Result<bool, String> {
        self.request_source_snapshot()
            .map(|snapshot| snapshot.required())
            .map_err(|error| error.describe())
    }
    fn request_source_revision(&self) -> Result<u64, String> {
        self.current_request_source_snapshot()
            .map(|snapshot| snapshot.generation())
            .map_err(|error| error.describe())
    }
    fn request_source_binding(&self) -> [u8; 32] {
        self.projected_request_source_snapshot().authority_binding()
    }
    fn request_source_catalog_binding(&self) -> [u8; 32] {
        self.projected_request_source_snapshot().catalog_binding()
    }
    fn read_request_source(
        &self,
        source: u64,
        offset: u64,
    ) -> Result<ContextSourceSlice, String> {
        self.request_source_snapshot()
            .and_then(|snapshot| self.service.read_request_source(&snapshot, source, offset))
            .map_err(|error| error.describe())
    }
    fn incorporated_revision(&self) -> u64 {
        self.projected_governing_state().incorporated
    }
    fn revision(&self) -> u64 {
        self.projected_governing_state().revision
    }
    fn render(&self) -> String {
        self.projected_governing_state().conversation
    }
    fn protected_paths(&self) -> Vec<PathBuf> {
        self.projected_governing_state().protected_paths
    }
    fn effective_permissions(&self) -> HostPermissions {
        self.projected_governing_state().permissions
    }
    fn permits_pipeline_handoff(&self) -> bool {
        self.projected_governing_state().permits_pipeline_handoff
    }
    fn checkpoint_before_workspace_mutation(
        &self,
        relative_path: &std::path::Path,
        kind: WorkspaceMutationKind,
    ) -> Result<(), String> {
        let start = self
            .workbench_start_record()
            .map_err(|error| format!("automatic workspace checkpoint is unavailable: {error}. Peritus did not change the workspace"))?;
        self.service
            .capture_automatic_checkpoint(&start, self.run_id, relative_path, kind)
            .map_err(|error| format!("automatic workspace checkpoint could not be durably captured: {error}. Peritus did not change the workspace"))
    }
    #[cfg(not(verus_only))]
    fn checkpoint_before_workspace_mutation_async<'a>(
        &'a self,
        relative_path: &'a std::path::Path,
        kind: WorkspaceMutationKind,
    ) -> peritus_product_runner::WorkspaceCheckpointFuture<'a> {
        Box::pin(self.capture_checkpoint_when_available(relative_path, kind))
    }
    fn checkpoint_workspace_mutation_from_baseline(
        &self,
        relative_path: &std::path::Path,
        kind: WorkspaceMutationKind,
        baseline: &WorkspaceMutationBaseline,
    ) -> Result<(), String> {
        let start = self
            .workbench_start_record()
            .map_err(|error| format!("automatic workspace checkpoint is unavailable: {error}. Peritus stopped because the completed command mutation has no durable before-image"))?;
        self.service
            .capture_automatic_checkpoint_from_baseline(
                &start,
                self.run_id,
                relative_path,
                kind,
                baseline,
            )
            .map_err(|error| format!("automatic workspace checkpoint could not publish the retained command baseline: {error}. Peritus stopped because the completed command mutation could not be recorded durably"))
    }
    fn seal_workspace_mutation_checkpoint(
        &self,
        relative_path: &std::path::Path,
        kind: WorkspaceMutationKind,
        owned_postchange: peritus_product_runner::control::CheckpointFileVersion,
    ) -> Result<(), String> {
        let start = self
            .workbench_start_record()
            .map_err(|error| format!("automatic workspace checkpoint is unavailable: {error}. Peritus stopped before accepting another workspace mutation"))?;
        self.service
            .seal_automatic_checkpoint(
                &start,
                self.run_id,
                &self.attempt_cancelled,
                relative_path,
                kind,
                owned_postchange,
            )
            .map_err(|error| format!("automatic workspace checkpoint could not be durably sealed: {error}. Peritus stopped because the completed mutation could not be recorded durably"))
    }
    #[cfg(not(verus_only))]
    fn interaction(&self) -> Option<&dyn DeveloperInteraction> {
        Some(self)
    }
}

impl ProductFindingBodyPublisher for LiveConversation {
    fn publish(
        &self,
        finding: &ProductFinding,
        source_ordinal: u64,
    ) -> Result<ProductFindingBodyReference, ProductReviewError> {
        self.service
            .inner
            .finding_bodies
            .publish(self.run_id, finding, source_ordinal)
    }

    fn publish_summary(
        &self,
        summary: &str,
        review_cycle: u32,
        source_ordinal: u64,
    ) -> Result<ProductReviewSummaryReference, ProductReviewError> {
        self.service.inner.finding_bodies.publish_summary(
            self.run_id,
            summary,
            review_cycle,
            source_ordinal,
        )
    }
}
