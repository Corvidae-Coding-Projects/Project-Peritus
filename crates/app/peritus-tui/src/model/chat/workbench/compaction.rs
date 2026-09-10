//! Explicit preview and confirmation for deterministic local source-handle reduction.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use peritus_app_protocol::{
    WellKnownProtocolFeature, WorkbenchCompactionFocus, WorkbenchCompactionPreview,
    WorkbenchCompactionRequest, WorkbenchIntent,
};

impl AppModel {
    pub(in crate::model::chat) fn compact_command(&mut self, focus: &str) -> Vec<Effect> {
        if !self.compaction_available() {
            self.notice(NoticeLevel::Warning, "Local compaction unavailable/offline; /reconnect or upgrade daemon. Draft retained.");
            return Vec::new();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(NoticeLevel::Warning, "Resolve the pending workbench request before previewing compaction; draft retained.");
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
                "Inspect a selected conversation with /sessions before compaction; draft retained.",
            );
            return Vec::new();
        };
        let focus = if focus.is_empty() {
            None
        } else {
            let Ok(value) = WorkbenchCompactionFocus::new(focus.to_owned()) else {
                self.notice(NoticeLevel::Warning, "Compaction focus must be 1–1024 bytes without terminal controls; draft retained.");
                return Vec::new();
            };
            Some(value)
        };
        let Ok(request) =
            WorkbenchCompactionRequest::new(snapshot.query(), snapshot.revision(), focus)
        else {
            return Vec::new();
        };
        self.chat.workbench.mode = WorkbenchMode::Compaction;
        self.chat.workbench.compaction_request = Some(request.clone());
        self.chat.workbench.compaction_preview = None;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.images.open = false;
        self.chat.workbench.files.open = false;
        self.chat.workbench.open = true;
        "Building a local deterministic source-handle preview; no remote compactor."
            .clone_into(&mut self.chat.workbench.message);
        self.request(
            AppRequestPayload::PreviewWorkbenchCompaction(request.clone()),
            PendingRequest::WorkbenchCompaction(request),
        )
        .into_iter()
        .collect()
    }

    pub(super) fn refresh_compaction(&mut self) -> Vec<Effect> {
        if !self.compaction_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(request) = self.chat.workbench.compaction_request.clone() else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::PreviewWorkbenchCompaction(request.clone()),
            PendingRequest::WorkbenchCompaction(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_compaction(
        &mut self,
        request: &WorkbenchCompactionRequest,
        preview: WorkbenchCompactionPreview,
    ) {
        if preview.request() != request
            || self.chat.workbench.compaction_request.as_ref() != Some(request)
            || self.chat.workbench.selected != Some(request.query())
            || self.chat.workbench.mode != WorkbenchMode::Compaction
        {
            return;
        }
        self.chat.workbench.compaction_preview = Some(preview);
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    pub(super) fn confirm_compaction(&mut self) -> Vec<Effect> {
        let Some(preview) = self.chat.workbench.compaction_preview.clone() else {
            return Vec::new();
        };
        if !preview.applicable() {
            self.notice(
                NoticeLevel::Info,
                "No safe byte-saving source-handle reduction is available; nothing changed.",
            );
            return Vec::new();
        }
        let workspace = preview.request().query().workspace();
        self.submit_workbench(WorkbenchIntent::ApplyCompaction(preview), workspace)
    }

    fn compaction_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchCompaction.as_str()
            })
    }
}
