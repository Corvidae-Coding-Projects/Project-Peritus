//! Workbench refresh routing, panel keys, feature gates, and pending-request detection.

use super::WorkbenchMode;
use crate::model::{AppModel, Effect, PendingRequest};
use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{AppRequestPayload, WellKnownProtocolFeature};

impl AppModel {
    pub(in crate::model) fn refresh_workbench(&mut self) -> Vec<Effect> {
        if !self.workbench_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        if self.chat.workbench.files.open {
            return self.refresh_file_panel();
        }
        if self.chat.workbench.goal_mode {
            return self.refresh_goal();
        }
        if self.chat.workbench.images.open && self.chat.workbench.images.list {
            return self.refresh_image_page(0, 0);
        }
        if self.chat.workbench.mode == WorkbenchMode::Brief {
            return self.refresh_brief();
        }
        if self.chat.workbench.mode == WorkbenchMode::Permissions {
            return self.refresh_permissions();
        }
        if matches!(self.chat.workbench.mode, WorkbenchMode::Init | WorkbenchMode::Memory) {
            return self.refresh_selected_snapshot();
        }
        if self.chat.workbench.mode == WorkbenchMode::Compaction {
            return self.refresh_compaction();
        }
        if self.chat.workbench.mode == WorkbenchMode::Checkpoints
            && self.chat.workbench.rewind_request.is_some()
        {
            return self.refresh_rewind();
        }
        if let Some(view) = self.chat.workbench.context_mode {
            return self.refresh_context(0, 0, view);
        }
        if self.chat.workbench.mode == WorkbenchMode::Queue {
            return self.refresh_queue(
                0,
                0,
                self.chat.workbench.queue.as_ref().is_some_and(|page| page.query().history()),
            );
        }
        self.refresh_selected_snapshot()
    }

    pub(super) fn refresh_selected_snapshot(&mut self) -> Vec<Effect> {
        if !self.workbench_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryWorkbench(query),
            PendingRequest::WorkbenchQuery(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model::chat) fn workbench_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if self.chat.workbench.files.open
            && let Some(effects) = self.file_key(key)
        {
            return effects;
        }
        if self.chat.workbench.images.open
            && let Some(effects) = self.image_key(key)
        {
            return effects;
        }
        if self.chat.workbench.mode == WorkbenchMode::Compaction && key.code == KeyCode::Char('c') {
            return self.confirm_compaction();
        }
        if self.chat.workbench.mode == WorkbenchMode::Checkpoints && key.code == KeyCode::Char('c')
        {
            return self.confirm_rewind();
        }
        match key.code {
            KeyCode::Esc => {
                if self.chat.workbench.mode == WorkbenchMode::Checkpoints {
                    self.chat.workbench.rewind_request = None;
                    self.chat.workbench.rewind_preview = None;
                }
                self.chat.workbench.open = false;
            }
            KeyCode::Up => {
                self.chat.workbench.scroll = self.chat.workbench.scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.chat.workbench.scroll =
                    self.chat.workbench.scroll.saturating_add(1).min(16384);
            }
            KeyCode::PageUp => {
                self.chat.workbench.scroll = self.chat.workbench.scroll.saturating_sub(5);
            }
            KeyCode::PageDown => {
                self.chat.workbench.scroll =
                    self.chat.workbench.scroll.saturating_add(5).min(16384);
            }
            KeyCode::Char('r') => return self.refresh_workbench(),
            _ => {}
        }
        Vec::new()
    }

    pub(in crate::model) fn workbench_available(&self) -> bool {
        self.context.is_some()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchControl.as_str()
            })
    }
    pub(super) fn library_available(&self) -> bool {
        self.context.is_some()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::ConversationLibrary.as_str()
            })
    }
    pub(in crate::model) fn workbench_request_pending(&self) -> bool {
        self.chat.workbench.files.reading.is_some()
            || self.chat.workbench.images.reading.is_some()
            || self.pending.values().any(|request| {
                matches!(
                    request,
                    PendingRequest::WorkbenchQuery(_)
                        | PendingRequest::ConversationLibrary(_)
                        | PendingRequest::WorkbenchImagePreview(_)
                        | PendingRequest::WorkbenchImages(_)
                        | PendingRequest::WorkbenchFilePreview(_)
                        | PendingRequest::WorkbenchFileImportPreview(_)
                        | PendingRequest::WorkbenchFileUpload { .. }
                        | PendingRequest::WorkbenchFiles(_)
                        | PendingRequest::WorkbenchReview(_)
                        | PendingRequest::WorkbenchImageUpload { .. }
                        | PendingRequest::WorkbenchContext(_)
                        | PendingRequest::WorkbenchCompaction(_)
                        | PendingRequest::WorkbenchCheckpointInspect(_)
                        | PendingRequest::WorkbenchRewind(_)
                        | PendingRequest::WorkbenchBrief(_)
                        | PendingRequest::WorkbenchGoal(_)
                        | PendingRequest::WorkbenchResult(_)
                        | PendingRequest::WorkbenchQueue(_)
                        | PendingRequest::WorkbenchControl(_)
                        | PendingRequest::WorkbenchReceipt(_)
                        | PendingRequest::WorkbenchPermissions(_)
                        | PendingRequest::WorkbenchInit(_)
                        | PendingRequest::WorkbenchMemory(_)
                )
            })
    }
}
