//! Focused diagnostic panel; opening or navigating it never starts inference.

use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest};
use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{
    AppRequestPayload, DoctorQuery, DoctorReport, WellKnownProtocolFeature,
};

#[derive(Debug)]
pub struct DoctorPanel {
    pub(crate) query: DoctorQuery,
    pub(crate) report: Option<DoctorReport>,
    pub(crate) scroll: usize,
    pub(crate) error: Option<String>,
}

impl AppModel {
    pub(super) fn doctor_command(&mut self) -> Vec<Effect> {
        if self.context.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Offline; diagnostic request not sent. Use /reconnect. Draft retained.",
            );
            return Vec::new();
        }
        if !self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::ProductDiagnostics.as_str()
        }) {
            self.notice(NoticeLevel::Warning, "This daemon did not negotiate product diagnostics. Upgrade the daemon to use /doctor; draft retained.");
            return Vec::new();
        }
        if self.pending.values().any(|pending| matches!(pending, PendingRequest::Doctor(_))) {
            self.notice(NoticeLevel::Info, "A diagnostic request is already pending.");
            return Vec::new();
        }
        let Some(workspace) = self.product.as_ref().map(|product| product.launch.workspace_id())
        else {
            self.notice(
                NoticeLevel::Warning,
                "Select a workspace before inspecting product diagnostics.",
            );
            return Vec::new();
        };
        let query = DoctorQuery::new(
            workspace,
            self.chat_providers().map(peritus_app_protocol::ProductProviderSelection::writer),
        );
        let Some(effect) =
            self.request(AppRequestPayload::Doctor(query), PendingRequest::Doctor(query))
        else {
            return Vec::new();
        };
        self.chat.doctor = Some(DoctorPanel { query, report: None, scroll: 0, error: None });
        vec![effect]
    }

    pub(super) fn doctor_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(panel) = self.chat.doctor.as_mut() else {
            return Vec::new();
        };
        match key.code {
            KeyCode::Esc => self.chat.doctor = None,
            KeyCode::Up => panel.scroll = panel.scroll.saturating_sub(1),
            KeyCode::Down => panel.scroll = panel.scroll.saturating_add(1).min(4096),
            KeyCode::PageUp => panel.scroll = panel.scroll.saturating_sub(5),
            KeyCode::PageDown => panel.scroll = panel.scroll.saturating_add(5).min(4096),
            KeyCode::Home => panel.scroll = 0,
            KeyCode::Char('r') => return self.doctor_command(),
            _ => {}
        }
        Vec::new()
    }

    pub(in crate::model) fn accept_doctor(&mut self, query: DoctorQuery, report: DoctorReport) {
        if query != report.query() {
            return;
        }
        if let Some(panel) = &mut self.chat.doctor
            && panel.query == query
        {
            panel.report = Some(report);
            panel.error = None;
            if self.chat.buffer.trim() == "/doctor" {
                self.clear_chat_command();
            }
            self.notice(
                NoticeLevel::Info,
                "Local diagnostic report received. No inference or repairs were run.",
            );
        }
    }
}
