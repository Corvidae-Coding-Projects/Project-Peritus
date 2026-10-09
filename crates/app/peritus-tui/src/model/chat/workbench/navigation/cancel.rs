//! Dismissing an inspection abandons its reads without losing an in-flight mutation receipt.

use crate::model::{AppModel, Effect, PendingRequest};
use peritus_app_protocol::{AppRequestPayload, ArtifactCancellation};

impl AppModel {
    pub(in crate::model) fn abandon_workbench_inspection(&mut self) {
        self.chat.workbench.snapshot_refresh_command = None;
        self.chat.workbench.goal_refresh_command = None;
        self.chat.workbench.goal_confirm_pending = None;
        self.chat.workbench.inspection_draft = None;
        self.chat.workbench.files.cancel_pending_preview();
        if self.workbench_chat_pending() {
            // These reads may be part of receipt reconciliation or a queued input's execution
            // admission. Retain the entire workflow until its accepted effects are known.
            return;
        }
        let requests: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(id, pending)| is_inspection(pending).then_some(*id))
            .collect();
        for request in requests {
            self.pending.remove(&request);
            self.pending_started.remove(&request);
        }
        // Dispatch owned transfer cleanup at the end of this model action, even if the caller
        // replaces the conversation state. Request order keeps cancellation after its upload.
        self.cancelled_imports.extend(self.chat.workbench.files.upload_binding());
        self.cancelled_imports.extend(self.chat.workbench.images.upload_binding());
        self.interrupt_file_import();
        self.interrupt_image_import();
    }

    pub(in crate::model) fn cancel_abandoned_imports(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        for (transfer, artifact) in std::mem::take(&mut self.cancelled_imports) {
            let Some(correlation) = self.ids.correlation() else { continue };
            effects.extend(self.request(
                AppRequestPayload::CancelArtifact(ArtifactCancellation::new(
                    transfer,
                    artifact,
                    correlation,
                )),
                PendingRequest::ArtifactCancel,
            ));
        }
        effects
    }
}

const fn is_inspection(pending: &PendingRequest) -> bool {
    matches!(
        pending,
        PendingRequest::WorkbenchQuery(_)
            | PendingRequest::WorkbenchExecution(_)
            | PendingRequest::ConversationLibrary(_)
            | PendingRequest::ResumeConversationLibrary(_)
            | PendingRequest::WorkbenchImagePreview(_)
            | PendingRequest::WorkbenchImages(_)
            | PendingRequest::WorkbenchFilePreview(_)
            | PendingRequest::WorkbenchFileImportPreview(_)
            | PendingRequest::WorkbenchFileUpload { .. }
            | PendingRequest::WorkbenchImageUpload { .. }
            | PendingRequest::WorkbenchFiles(_)
            | PendingRequest::WorkbenchReview(_)
            | PendingRequest::WorkbenchReviewSummary(_)
            | PendingRequest::WorkbenchReviewDiff(_)
            | PendingRequest::WorkbenchReviewDiffBytes(_)
            | PendingRequest::WorkbenchContext(_)
            | PendingRequest::WorkbenchQueue(_)
            | PendingRequest::WorkbenchQueueCommand { .. }
            | PendingRequest::WorkbenchCompaction(_)
            | PendingRequest::WorkbenchCheckpointInspect(_)
            | PendingRequest::WorkbenchRewind(_)
            | PendingRequest::WorkbenchCheckpointPage(_)
            | PendingRequest::WorkbenchRewindPage(_)
            | PendingRequest::WorkbenchBrief(_)
            | PendingRequest::WorkbenchGoal(_)
            | PendingRequest::WorkbenchResult(_)
            | PendingRequest::WorkbenchPermissions(_)
            | PendingRequest::WorkbenchInit(_)
            | PendingRequest::WorkbenchMemory(_)
    )
}
