//! Checkpoint creation and rewind request commands.

use super::{AppModel, Effect, NoticeLevel};
use crate::model::{PendingRequest, decode_hex_16};
use peritus_app_protocol::{
    AppRequestPayload, ControlOperationId, WorkbenchCheckpointName, WorkbenchIntent,
    WorkbenchRewindRequest,
};

impl AppModel {
    pub(in crate::model::chat) fn checkpoint_command(&mut self, name: &str) -> Vec<Effect> {
        if !self.checkpoints_available() {
            self.notice(
                NoticeLevel::Warning,
                "Checkpoints unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before creating a checkpoint; draft retained.",
            );
            return Vec::new();
        }
        if let Some(checkpoint) = name.strip_prefix("show")
            && (checkpoint.is_empty() || checkpoint.chars().next().is_some_and(char::is_whitespace))
        {
            return self.inspect_checkpoint(checkpoint.trim());
        }
        let Some(snapshot) = self
            .chat
            .workbench
            .snapshot
            .as_ref()
            .filter(|snapshot| self.chat.workbench.selected == Some(snapshot.query()))
        else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect a selected conversation with /sessions before creating a checkpoint; draft retained.",
            );
            return Vec::new();
        };
        let chosen = if name.is_empty() {
            format!("Checkpoint at revision {}", snapshot.revision())
        } else {
            name.to_owned()
        };
        let Ok(name) = WorkbenchCheckpointName::new(chosen) else {
            self.notice(
                NoticeLevel::Warning,
                "Checkpoint name must be 1–256 bytes without control characters; draft retained.",
            );
            return Vec::new();
        };
        let workspace = snapshot.query().workspace();
        self.open_checkpoint_panel();
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.checkpoint_page = None;
        self.chat.workbench.checkpoint_page_request = None;
        self.chat.workbench.checkpoint_page_history.clear();
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.rewind_page = None;
        self.chat.workbench.rewind_page_request = None;
        self.chat.workbench.rewind_page_history.clear();
        self.chat.workbench.restore_receipt = None;
        self.chat.workbench.restore_summary = None;
        "Capturing selected whole workspace files and explicit exclusions at this boundary."
            .clone_into(&mut self.chat.workbench.message);
        self.submit_workbench(WorkbenchIntent::CreateCheckpoint(name), workspace)
    }

    fn inspect_checkpoint(&mut self, checkpoint: &str) -> Vec<Effect> {
        let Some(snapshot) = self
            .chat
            .workbench
            .snapshot
            .as_ref()
            .filter(|snapshot| self.chat.workbench.selected == Some(snapshot.query()))
        else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect a selected conversation with /sessions before loading a checkpoint; draft retained.",
            );
            return Vec::new();
        };
        let Some(checkpoint) =
            decode_hex_16(checkpoint).and_then(|bytes| ControlOperationId::new(bytes).ok())
        else {
            self.notice(
                NoticeLevel::Warning,
                "Use /checkpoint show <32 hexadecimal checkpoint ID>; draft retained.",
            );
            return Vec::new();
        };
        let Ok(request) =
            WorkbenchRewindRequest::new(snapshot.query(), snapshot.revision(), checkpoint)
        else {
            return Vec::new();
        };
        self.open_checkpoint_panel();
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.checkpoint_page = None;
        self.chat.workbench.checkpoint_page_request = None;
        self.chat.workbench.checkpoint_page_history.clear();
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.rewind_page = None;
        self.chat.workbench.rewind_page_request = None;
        self.chat.workbench.rewind_page_history.clear();
        self.chat.workbench.restore_receipt = None;
        self.chat.workbench.restore_summary = None;
        "Loading the exact persisted checkpoint and its historical references."
            .clone_into(&mut self.chat.workbench.message);
        if self.checkpoint_pages_available()
            && let Ok(page_request) = peritus_app_protocol::WorkbenchCheckpointPageRequest::new(
                request.query(),
                request.revision(),
                request.checkpoint(),
                None,
            )
        {
            self.chat.workbench.checkpoint_page_request = Some(page_request);
            self.chat.workbench.checkpoint_page_history = vec![None];
            return self
                .request(
                    AppRequestPayload::QueryWorkbenchCheckpointPage(page_request),
                    PendingRequest::WorkbenchCheckpointPage(page_request),
                )
                .into_iter()
                .collect();
        }
        self.request(
            AppRequestPayload::InspectWorkbenchCheckpoint(request),
            PendingRequest::WorkbenchCheckpointInspect(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model::chat) fn rewind_command(&mut self, checkpoint: &str) -> Vec<Effect> {
        let parts: Vec<_> = checkpoint.split_whitespace().collect();
        let checkpoint = parts.first().copied().unwrap_or("");
        let mode = match parts.get(1).copied().unwrap_or("files") {
            "files" if parts.len() <= 2 => peritus_app_protocol::WorkbenchRewindMode::FilesOnly,
            "conversation" => peritus_app_protocol::WorkbenchRewindMode::ConversationOnly,
            "combined" => peritus_app_protocol::WorkbenchRewindMode::Combined,
            _ => {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /rewind <checkpoint-id> [files|conversation|combined]. Draft retained.",
                );
                return Vec::new();
            }
        };
        if parts.len() > 2 {
            self.notice(
                NoticeLevel::Warning,
                "Use /rewind <checkpoint-id> [files|conversation|combined]. Draft retained.",
            );
            return Vec::new();
        }
        if !self.checkpoints_available() {
            self.notice(
                NoticeLevel::Warning,
                "Checkpoint rewind unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before previewing rewind; draft retained.",
            );
            return Vec::new();
        }
        let Some(snapshot) = self
            .chat
            .workbench
            .snapshot
            .as_ref()
            .filter(|snapshot| self.chat.workbench.selected == Some(snapshot.query()))
        else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect a selected conversation with /sessions before rewind; draft retained.",
            );
            return Vec::new();
        };
        let Some(checkpoint) =
            decode_hex_16(checkpoint).and_then(|bytes| ControlOperationId::new(bytes).ok())
        else {
            self.notice(
                NoticeLevel::Warning,
                "Use /rewind <32 hexadecimal checkpoint ID>; draft retained.",
            );
            return Vec::new();
        };
        let Ok(request) =
            WorkbenchRewindRequest::new(snapshot.query(), snapshot.revision(), checkpoint)
        else {
            return Vec::new();
        };
        let request = if mode == peritus_app_protocol::WorkbenchRewindMode::FilesOnly {
            request
        } else {
            let Ok(child) = peritus_app_protocol::ConversationId::new(
                self.ids.bytes(b"workbench-rewind-conversation"),
            ) else {
                return Vec::new();
            };
            let Ok(request) = request.with_branch(mode, child) else {
                return Vec::new();
            };
            request
        };
        self.request_rewind_preview(request)
    }

    fn request_rewind_preview(&mut self, request: WorkbenchRewindRequest) -> Vec<Effect> {
        self.open_checkpoint_panel();
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.checkpoint_page = None;
        self.chat.workbench.checkpoint_page_request = None;
        self.chat.workbench.checkpoint_page_history.clear();
        self.chat.workbench.rewind_request = Some(request);
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.rewind_page = None;
        self.chat.workbench.rewind_page_request = None;
        self.chat.workbench.rewind_page_history.clear();
        self.chat.workbench.restore_receipt = None;
        self.chat.workbench.restore_summary = None;
        "Inspecting current covered bytes; this preview cannot change the workspace."
            .clone_into(&mut self.chat.workbench.message);
        if self.checkpoint_pages_available() {
            let page_request = peritus_app_protocol::WorkbenchRewindPageRequest::new(request, None);
            self.chat.workbench.rewind_page_request = Some(page_request);
            self.chat.workbench.rewind_page_history = vec![None];
            return self
                .request(
                    AppRequestPayload::QueryWorkbenchRewindPage(page_request),
                    PendingRequest::WorkbenchRewindPage(page_request),
                )
                .into_iter()
                .collect();
        }
        self.request(
            AppRequestPayload::PreviewWorkbenchRewind(request),
            PendingRequest::WorkbenchRewind(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model::chat::workbench) fn refresh_rewind(&mut self) -> Vec<Effect> {
        if !self.checkpoints_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(request) = self.chat.workbench.rewind_request else { return Vec::new() };
        let mode = match request.mode() {
            peritus_app_protocol::WorkbenchRewindMode::FilesOnly => "files",
            peritus_app_protocol::WorkbenchRewindMode::ConversationOnly => "conversation",
            peritus_app_protocol::WorkbenchRewindMode::Combined => "combined",
        };
        let command =
            format!("/rewind {} {mode}", crate::model::format_id(request.checkpoint().as_bytes()));
        self.chat.workbench.rewind_preview = None;
        self.refresh_snapshot_for_command(request.query(), command)
    }
}
