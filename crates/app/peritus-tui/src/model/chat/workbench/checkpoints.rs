//! Explicit bounded checkpoint creation and preview-bound rewind confirmation.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use crate::model::decode_hex_16;
use peritus_app_protocol::{
    ControlOperationId, WellKnownProtocolFeature, WorkbenchCheckpointName,
    WorkbenchCheckpointReceipt, WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreReceipt,
    WorkbenchRestoreStatus, WorkbenchRewindPreview, WorkbenchRewindRequest,
};
mod receipts;

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
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
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
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
        "Loading the exact persisted checkpoint and its historical references."
            .clone_into(&mut self.chat.workbench.message);
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
                self.notice(NoticeLevel::Warning, "Use /rewind <checkpoint-id> [files|conversation|combined] [time=<ms> requests=<n> tools=<n> tokens=<n>]. Draft retained.");
                return Vec::new();
            }
        };
        let allocation = if parts.len() > 2 {
            let Some(budget) = super::fork::parse_fork_budget(&parts[2..]) else {
                self.notice(
                    NoticeLevel::Warning,
                    "A logical rewind budget requires all four allocation fields; draft retained.",
                );
                return Vec::new();
            };
            Some(budget)
        } else {
            None
        };
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
            let Ok(request) = request.with_branch(mode, child, allocation) else {
                return Vec::new();
            };
            request
        };
        self.open_checkpoint_panel();
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.rewind_request = Some(request);
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
        "Inspecting current covered bytes; this preview cannot change the workspace."
            .clone_into(&mut self.chat.workbench.message);
        self.request(
            AppRequestPayload::PreviewWorkbenchRewind(request),
            PendingRequest::WorkbenchRewind(request),
        )
        .into_iter()
        .collect()
    }

    pub(super) fn refresh_rewind(&mut self) -> Vec<Effect> {
        if !self.checkpoints_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(request) = self.chat.workbench.rewind_request else { return Vec::new() };
        self.request(
            AppRequestPayload::PreviewWorkbenchRewind(request),
            PendingRequest::WorkbenchRewind(request),
        )
        .into_iter()
        .collect()
    }

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
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    pub(super) fn confirm_rewind(&mut self) -> Vec<Effect> {
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
}
