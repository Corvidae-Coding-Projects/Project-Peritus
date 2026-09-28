//! Read-only session paging and exact keyboard selection; opening never starts execution.

use super::WorkbenchMode;
use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest};
use crossterm::event::KeyCode;
use peritus_app_protocol::{AppRequestPayload, ConversationLibraryPage, ConversationLibraryQuery};

impl AppModel {
    pub(super) fn library_key(&mut self, key: KeyCode) -> Option<Vec<Effect>> {
        match key {
            KeyCode::Char('n') => {
                let offset = self.chat.workbench.library.as_ref()?.next_offset();
                Some(offset.map_or_else(Vec::new, |offset| self.refresh_library_page(Some(offset))))
            }
            KeyCode::Char('p') => {
                let query = self.chat.workbench.library.as_ref()?.query();
                let offset = query.offset().saturating_sub(u32::from(query.limit()));
                Some(if offset == query.offset() {
                    Vec::new()
                } else {
                    self.refresh_library_page(Some(offset))
                })
            }
            KeyCode::Enter => Some(self.open_library_selection()),
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {
                let length = self.chat.workbench.library.as_ref()?.items().len();
                if length == 0 {
                    return None;
                }
                let selected = self.chat.workbench.library_selected;
                self.chat.workbench.library_selected = match key {
                    KeyCode::Up => selected.saturating_sub(1),
                    KeyCode::Down => selected.saturating_add(1).min(length.saturating_sub(1)),
                    KeyCode::End => length.saturating_sub(1),
                    _ => 0,
                };
                self.chat.workbench.scroll = crate::render::library_selection_scroll(self);
                Some(Vec::new())
            }
            _ => None,
        }
    }

    pub(super) fn refresh_library_page(&mut self, offset: Option<u32>) -> Vec<Effect> {
        if !self.library_available() || self.workbench_request_pending() {
            self.notice(
                NoticeLevel::Info,
                "Session library is offline or waiting for a reply; current page retained.",
            );
            return Vec::new();
        }
        let Some(previous) = &self.chat.workbench.library_query else { return Vec::new() };
        let Ok(query) = ConversationLibraryQuery::new(
            previous.workspace(),
            previous.literal().cloned(),
            previous.include_archived(),
            offset.unwrap_or_else(|| previous.offset()),
            previous.limit(),
        ) else {
            return Vec::new();
        };
        self.chat.workbench.library_query = Some(query.clone());
        self.request(
            AppRequestPayload::QueryConversationLibrary(query.clone()),
            PendingRequest::ConversationLibrary(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_library_page(&mut self, page: &ConversationLibraryPage) {
        let panel = &mut self.chat.workbench;
        let old = panel
            .library
            .as_ref()
            .filter(|old| old.query() == page.query())
            .and_then(|old| old.items().get(panel.library_selected))
            .map(peritus_app_protocol::ConversationLibraryItem::query);
        panel.library_selected = old
            .and_then(|query| page.items().iter().position(|item| item.query() == query))
            .unwrap_or(0);
        panel.library = Some(page.clone());
        panel.library_query = Some(page.query().clone());
        panel.message = format!(
            "{} matching conversations. Opening a session does not start work.",
            page.total()
        );
        self.complete_workbench_inspection();
        self.chat.workbench.scroll = crate::render::library_selection_scroll(self);
    }

    fn open_library_selection(&mut self) -> Vec<Effect> {
        if self.chat_mutation_pending()
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
        {
            self.notice(
                NoticeLevel::Info,
                "Waiting for the pending receipt or page before changing conversations.",
            );
            return Vec::new();
        }
        let Some(item) = self
            .chat
            .workbench
            .library
            .as_ref()
            .and_then(|page| page.items().get(self.chat.workbench.library_selected))
        else {
            return Vec::new();
        };
        let query = item.query();
        let legacy = item.legacy_run();
        if self
            .product
            .as_ref()
            .is_some_and(|product| product.launch.workspace_id() != query.workspace())
        {
            if !self.chat.buffer.is_empty() {
                self.notice(
                    NoticeLevel::Warning,
                    "Send or clear this workspace's draft before opening another workspace.",
                );
                return Vec::new();
            }
            return vec![legacy.map_or(Effect::OpenConversation(query), |run| Effect::OpenRun {
                run,
                workspace: query.workspace(),
            })];
        }
        self.abandon_chat_observations();
        self.select_workbench_conversation(if legacy.is_some() { None } else { Some(query) });
        self.chat.workbench.mode = WorkbenchMode::Sessions;
        self.chat.workbench.scroll = 0;
        if let Some(run) = legacy {
            self.chat.workbench.open = false;
            self.chat.run_id = Some(run);
            self.chat.snapshot = None;
            self.query_chat_binding(run, true).into_iter().collect()
        } else {
            self.discover_workbench_execution()
        }
    }
}
