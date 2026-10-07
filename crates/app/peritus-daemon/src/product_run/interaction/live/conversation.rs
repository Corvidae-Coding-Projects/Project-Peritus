//! Read-only conversation projection and daemon-owned source access.

use super::{LiveConversation, ProductRunServiceError};
#[cfg(not(verus_only))]
use peritus_agent::DeveloperInteraction;
use peritus_product_runner::{
    ContextSourcePage, ContextSourceSlice, ConversationView, WorkspaceMutationKind,
    control::HostPermissions,
};
use peritus_review::{
    ProductFinding, ProductFindingBodyPublisher, ProductFindingBodyReference,
    ProductReviewError, ProductReviewSummaryReference,
};
use std::path::PathBuf;

impl ConversationView for LiveConversation {
    fn uses_explicit_media(&self) -> bool {
        // An unavailable control binding must not permit a fallback to ambient file discovery.
        self.service.governed_run(self.run_id).unwrap_or(true)
    }
    fn stable_request_context(&self) -> String {
        let result = (|| {
            let record = self.attempt_record()?;
            let start = record.interaction.workbench;
            self.service.with_control_conversation(start.conversation(), |store| {
                store.capture_execution(&start)?;
                let record = store.load(start.conversation())?.ok_or(peritus_product_runner::control::ControlError::NotFound)?;
                let text = record.inputs().incorporated_conversation()?;
                Ok(if text.is_empty() { "Current user instructions are supplied by the host at the request admission boundary.".to_owned() } else { text })
            }).map_err(ProductRunServiceError::from)
        })();
        result.unwrap_or_else(|_| {
            "Governing conversation unavailable; execution must stop.".to_owned()
        })
    }
    fn reference_authority_context(&self) -> String {
        let result = (|| {
            let record = self.attempt_record()?;
            self.service
                .record_capture(&record)
                .map(|capture| capture.reference_authority_context().to_owned())
        })();
        result.unwrap_or_default()
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
        self.request_source_snapshot()
            .map_or([0_u8; 32], |snapshot| snapshot.authority_binding())
    }
    fn request_source_catalog_binding(&self) -> [u8; 32] {
        self.request_source_snapshot()
            .map_or([0_u8; 32], |snapshot| snapshot.catalog_binding())
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
        self.attempt_record()
            .map(|record| record.interaction.incorporated)
            .unwrap_or(0)
    }
    fn revision(&self) -> u64 {
        self.attempt_record()
            .and_then(|record| self.service.record_input_revision(&record))
            .unwrap_or(u64::MAX)
    }
    fn render(&self) -> String {
        self.attempt_record()
            .and_then(|record| self.service.record_input(&record))
            .map_or_else(
                || "Governing conversation unavailable; execution must stop.".to_owned(),
                |input| input.conversation,
            )
    }
    fn protected_paths(&self) -> Vec<PathBuf> {
        self.review_record()
            // An unavailable narrowing record must prevent mutation while retaining read-only
            // diagnosis. Empty relative path means the complete workspace mutation surface.
            .map_or_else(|_| vec![PathBuf::new()], |record| record.reviews().protected_paths())
    }
    fn effective_permissions(&self) -> HostPermissions {
        // Tool boundaries must not retain ambient authority when the durable policy cannot be
        // read or its run/workspace binding is unavailable.
        self.service.effective_permissions(self.run_id).unwrap_or_else(|_| HostPermissions::none())
    }
    fn permits_pipeline_handoff(&self) -> bool {
        self.review_record().is_ok_and(|record| {
            record.reviews().pending_pipeline_permission(record.inputs()).unwrap_or(true)
        })
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
