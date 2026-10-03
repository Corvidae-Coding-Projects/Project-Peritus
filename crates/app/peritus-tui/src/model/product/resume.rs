//! One-shot workspace-scoped selection for `peritus resume`.

use peritus_app_protocol::{
    AppRequestPayload, ConversationLibraryItem, ConversationLibraryPage, ConversationLibraryQuery,
    WorkbenchQuery,
};

use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest};

#[derive(Debug, Default)]
pub(super) struct LatestConversation {
    newest: Option<(u64, WorkbenchQuery)>,
}

impl LatestConversation {
    fn observe(&mut self, item: &ConversationLibraryItem) {
        let candidate = (item.activity_revision(), item.query());
        if self.newest.is_none_or(|current| candidate.0 > current.0) {
            self.newest = Some(candidate);
        }
    }
}

impl AppModel {
    pub(in crate::model) fn resume_latest_pending(&self) -> bool {
        self.product.as_ref().is_some_and(|product| product.resume.is_some())
    }

    pub(in crate::model) fn resume_latest_conversation(&mut self) -> Vec<Effect> {
        if !self.resume_latest_pending() {
            return Vec::new();
        }
        if self.chat.workbench.selected.is_some() || self.chat.run_id.is_some() {
            self.cancel_latest_resume();
            return Vec::new();
        }
        if !self.library_available() || !self.workbench_available() {
            self.cancel_latest_resume();
            self.notice(
                NoticeLevel::Error,
                "This daemon cannot find the latest saved conversation. Start a new conversation or upgrade and run peritus resume again.",
            );
            return Vec::new();
        }
        if self.workbench_request_pending() {
            return Vec::new();
        }
        self.notice(NoticeLevel::Info, "Finding this folder's most recent conversation…");
        self.request_latest_page(0)
    }

    fn request_latest_page(&mut self, offset: u32) -> Vec<Effect> {
        let Some(workspace) = self.product.as_ref().map(|product| product.launch.workspace_id())
        else {
            return Vec::new();
        };
        let Ok(query) = ConversationLibraryQuery::new(workspace, None, true, offset, 64) else {
            self.cancel_latest_resume();
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryConversationLibrary(query.clone()),
            PendingRequest::ResumeConversationLibrary(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_latest_page(
        &mut self,
        page: &ConversationLibraryPage,
    ) -> Vec<Effect> {
        let Some(workspace) = self.product.as_ref().map(|product| product.launch.workspace_id())
        else {
            return Vec::new();
        };
        if self.chat.workbench.selected.is_some() || self.chat.run_id.is_some() {
            self.cancel_latest_resume();
            return Vec::new();
        }
        if let Some(state) = self.product.as_mut().and_then(|product| product.resume.as_mut()) {
            for item in page.items() {
                if item.query().workspace() == workspace {
                    state.observe(item);
                }
            }
        } else {
            return Vec::new();
        }
        if let Some(offset) = page.next_offset() {
            return self.request_latest_page(offset);
        }
        let newest = self
            .product
            .as_mut()
            .and_then(|product| product.resume.take())
            .and_then(|state| state.newest);
        let Some((_, query)) = newest else {
            self.notice(
                NoticeLevel::Info,
                "No saved conversation exists for this folder. Started a new conversation.",
            );
            return Vec::new();
        };
        self.chat.workbench.open = true;
        self.select_workbench_conversation(Some(query));
        self.notice(NoticeLevel::Info, "Opening this folder's most recent conversation…");
        self.discover_workbench_execution()
    }

    pub(in crate::model) fn fail_latest_resume(&mut self, detail: &str) {
        self.cancel_latest_resume();
        self.notice(
            NoticeLevel::Error,
            format!(
                "Could not find this folder's most recent conversation: {detail}. Start a new conversation or run peritus resume again."
            ),
        );
    }

    pub(in crate::model) const fn cancel_latest_resume(&mut self) {
        if let Some(product) = &mut self.product {
            product.resume = None;
        }
    }
}
