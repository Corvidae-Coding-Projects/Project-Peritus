//! Explicit bounded checkpoint creation and preview-bound rewind confirmation.

use super::{AppModel, Effect, NoticeLevel, WorkbenchMode};
use peritus_app_protocol::{
    WellKnownProtocolFeature, WorkbenchCheckpointReceipt, WorkbenchCommand, WorkbenchIntent,
    WorkbenchRestoreReceipt, WorkbenchRestoreStatus, WorkbenchRewindPreview,
    WorkbenchRewindRequest,
};
mod commands;
mod pages;
mod receipts;

impl AppModel {
    pub(in crate::model) fn accept_workbench_rewind(
        &mut self,
        request: &WorkbenchRewindRequest,
        preview: WorkbenchRewindPreview,
    ) {
        if preview.request() != *request
            || self.chat.workbench.rewind_request.as_ref() != Some(request)
            || self.chat.workbench.selected != Some(request.query())
            || self.chat.workbench.mode != WorkbenchMode::Checkpoints
        {
            return;
        }
        self.chat.workbench.rewind_preview = Some(preview);
        self.complete_workbench_inspection();
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    pub(in crate::model) fn accept_workbench_checkpoint_page(
        &mut self,
        request: peritus_app_protocol::WorkbenchCheckpointPageRequest,
        page: peritus_app_protocol::WorkbenchCheckpointCoveragePage,
    ) {
        let page_selected_revision = page.selected_revision();
        let page_accepted_revision = page.accepted_revision();
        let requested_revision = request.revision();
        let cursor_matches = request.cursor().map_or_else(
            || page.offset() == 0,
            |cursor| {
                page.section() == cursor.section()
                    && page.offset() == cursor.offset()
                    && page.fingerprint() == cursor.fingerprint()
            },
        );
        if page.checkpoint() != request.checkpoint()
            || page.query() != request.query()
            || page_selected_revision != requested_revision
            || page_accepted_revision > requested_revision
            || !cursor_matches
            || self.chat.workbench.selected != Some(request.query())
            || self.chat.workbench.mode != WorkbenchMode::Checkpoints
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched checkpoint coverage page; no page accepted.",
            );
            return;
        }
        self.chat.workbench.checkpoint_page_request = Some(request);
        self.chat.workbench.checkpoint_page = Some(page);
        self.chat.workbench.checkpoint_receipt = None;
        self.complete_workbench_inspection();
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    pub(in crate::model) fn accept_workbench_checkpoint_command_page(
        &mut self,
        command: &WorkbenchCommand,
        page: &peritus_app_protocol::WorkbenchCheckpointCoveragePage,
    ) -> Vec<Effect> {
        let WorkbenchIntent::CreateCheckpoint(name) = command.intent() else { return Vec::new() };
        let page_checkpoint = page.checkpoint();
        let command_operation = command.operation();
        if page_checkpoint != command_operation
            || page.query() != command.query()
            || page.accepted_revision() != command.expected_revision().saturating_add(1)
            || page.selected_revision() != page.accepted_revision()
            || page.name() != name
            || page.offset() != 0
            || !self
                .chat
                .workbench
                .unresolved
                .as_ref()
                .is_some_and(|(expected, _)| expected == command)
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched checkpoint creation page; no page accepted.",
            );
            return Vec::new();
        }
        self.clear_checkpoint_command(command);
        self.open_checkpoint_panel();
        self.chat.workbench.selected = Some(page.query());
        self.chat.workbench.receipted_revision =
            self.chat.workbench.receipted_revision.max(page.accepted_revision());
        self.chat.workbench.snapshot = None;
        self.chat.workbench.checkpoint_page_request = Some(
            peritus_app_protocol::WorkbenchCheckpointPageRequest::new(
                page.query(),
                page.selected_revision(),
                page.checkpoint(),
                None,
            )
            .expect("validated page revision is positive"),
        );
        self.chat.workbench.checkpoint_page_history = vec![None];
        self.chat.workbench.checkpoint_page = Some(page.clone());
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.rewind_page = None;
        self.chat.workbench.restore_receipt = None;
        self.chat.workbench.restore_summary = None;
        self.chat.workbench.message = format!(
            "Checkpoint durably captured: {} covered path(s), {} exclusion(s), and {} external-effect fact(s).",
            page.total_paths(),
            page.total_exclusions(),
            page.total_external_effects(),
        );
        self.refresh_workbench()
    }

    pub(in crate::model) fn accept_workbench_rewind_page(
        &mut self,
        request: peritus_app_protocol::WorkbenchRewindPageRequest,
        page: peritus_app_protocol::WorkbenchRewindCoveragePage,
    ) {
        let confirmation = page.confirmation();
        let cursor_matches = request.cursor().map_or_else(
            || page.offset() == 0,
            |cursor| {
                let preview_digest = confirmation.preview_digest();
                let cursor_fingerprint = cursor.fingerprint();
                page.section() == cursor.section()
                    && page.offset() == cursor.offset()
                    && preview_digest == cursor_fingerprint
            },
        );
        if confirmation.request() != request.request()
            || self.chat.workbench.rewind_request != Some(request.request())
            || self.chat.workbench.selected != Some(request.request().query())
            || self.chat.workbench.mode != WorkbenchMode::Checkpoints
            || !cursor_matches
        {
            self.notice(NoticeLevel::Error, "Mismatched rewind coverage page; no page accepted.");
            return;
        }
        self.chat.workbench.rewind_page_request = Some(request);
        self.chat.workbench.rewind_page = Some(page);
        self.complete_workbench_inspection();
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    pub(super) fn confirm_rewind(&mut self) -> Vec<Effect> {
        if let Some(page) = self.chat.workbench.rewind_page.clone() {
            let confirmation = page.confirmation();
            return self.submit_workbench(
                WorkbenchIntent::ConfirmRewind(confirmation),
                confirmation.request().query().workspace(),
            );
        }
        let Some(preview) = self.chat.workbench.rewind_preview.clone() else {
            return Vec::new();
        };
        self.submit_workbench(
            WorkbenchIntent::ApplyRewind(preview.clone()),
            preview.request().query().workspace(),
        )
    }

    fn clear_checkpoint_command(&mut self, command: &WorkbenchCommand) {
        if let Some((_, draft)) = self.chat.workbench.unresolved.take()
            && self.chat.buffer == draft
        {
            self.clear_chat_command();
        }
        debug_assert_eq!(self.chat.workbench.selected, Some(command.query()));
    }

    pub(super) fn rewind_binding(
        &mut self,
        preview: &WorkbenchRewindPreview,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(peritus_app_protocol::WorkbenchQuery, u64)> {
        if self.chat.workbench.mode != WorkbenchMode::Checkpoints
            || self.chat.workbench.rewind_preview.as_ref() != Some(preview)
            || self.chat.workbench.selected != Some(preview.request().query())
            || preview.request().query().workspace() != workspace
        {
            self.notice(
                NoticeLevel::Warning,
                "Preview /rewind before confirming an exact restore; draft retained.",
            );
            return None;
        }
        Some((preview.request().query(), preview.request().revision()))
    }

    pub(super) fn rewind_confirmation_binding(
        &mut self,
        confirmation: peritus_app_protocol::WorkbenchRewindConfirmation,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(peritus_app_protocol::WorkbenchQuery, u64)> {
        if self.chat.workbench.mode != WorkbenchMode::Checkpoints
            || self
                .chat
                .workbench
                .rewind_page
                .as_ref()
                .map(peritus_app_protocol::WorkbenchRewindCoveragePage::confirmation)
                != Some(confirmation)
            || self.chat.workbench.selected != Some(confirmation.request().query())
            || confirmation.request().query().workspace() != workspace
        {
            self.notice(
                NoticeLevel::Warning,
                "Load a full-checkpoint rewind page before confirming; draft retained.",
            );
            return None;
        }
        Some((confirmation.request().query(), confirmation.request().revision()))
    }

    const fn open_checkpoint_panel(&mut self) {
        self.chat.workbench.mode = WorkbenchMode::Checkpoints;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.images.open = false;
        self.chat.workbench.files.open = false;
        self.chat.workbench.open = true;
        self.chat.workbench.scroll = 0;
    }

    fn checkpoints_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchCheckpoints.as_str()
            })
    }

    fn checkpoint_pages_available(&self) -> bool {
        self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::WorkbenchCheckpointPages.as_str()
        })
    }
}
