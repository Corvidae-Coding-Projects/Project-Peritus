//! Revision-fenced retained-image inspection and explicit future-selection changes.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchIntent};
use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{WorkbenchImagePage, WorkbenchImageQuery, WorkbenchQuery};
use peritus_types::WorkspaceId;

impl AppModel {
    pub(in crate::model::chat::workbench) fn refresh_image_page(
        &mut self,
        revision: u64,
        offset: u32,
    ) -> Vec<Effect> {
        if !self.images_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(scope) = self.chat.workbench.selected else { return Vec::new() };
        let Ok(query) = WorkbenchImageQuery::new(scope, revision, offset) else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryWorkbenchImages(query),
            PendingRequest::WorkbenchImages(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_images(
        &mut self,
        query: WorkbenchImageQuery,
        page: WorkbenchImagePage,
    ) {
        if self.chat.workbench.selected != Some(query.query())
            || page.query().query() != query.query()
            || page.query().offset() != query.offset()
            || (query.revision() != 0 && page.query().revision() != query.revision())
        {
            self.chat.workbench.images.page = None;
            self.notice(
                NoticeLevel::Error,
                "Image page does not match the requested scope/revision; refresh before selecting.",
            );
            return;
        }
        let image = &mut self.chat.workbench.images;
        image.selected = 0;
        image.page = Some(page);
        self.chat.workbench.scroll = 0;
    }

    pub(in crate::model::chat::workbench) fn image_page_binding(
        &mut self,
        workspace: WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let binding = self
            .chat
            .workbench
            .images
            .page
            .as_ref()
            .filter(|page| {
                self.chat.workbench.images.open
                    && self.chat.workbench.images.list
                    && page.query().query().workspace() == workspace
                    && self.chat.workbench.selected == Some(page.query().query())
            })
            .map(|page| (page.query().query(), page.query().revision()));
        if binding.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Inspect the current image page before changing selection; draft retained.",
            );
        }
        binding
    }

    pub(super) fn image_page_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        if key.code == KeyCode::Char('i') {
            self.chat.workbench.images.list = false;
            self.chat.workbench.scroll = 0;
            return Some(self.refresh_workbench());
        }
        let image = &mut self.chat.workbench.images;
        let page = image.page.as_ref()?;
        match key.code {
            KeyCode::Left => image.selected = image.selected.saturating_sub(1),
            KeyCode::Right => {
                image.selected =
                    image.selected.saturating_add(1).min(page.rows().len().saturating_sub(1));
            }
            KeyCode::Char(' ') => {
                let row = page.rows().get(image.selected)?;
                let intent = WorkbenchIntent::SelectImage {
                    attachment: row.operation(),
                    selected: !row.selected(),
                };
                let workspace = page.query().query().workspace();
                return Some(self.submit_workbench(intent, workspace));
            }
            KeyCode::Char('n' | 'b') => {
                let count = u32::try_from(peritus_app_protocol::MAX_WORKBENCH_IMAGE_PAGE).ok()?;
                let offset = if key.code == KeyCode::Char('n') {
                    let next = page.query().offset().saturating_add(count);
                    if next >= page.total() {
                        return Some(Vec::new());
                    }
                    next
                } else {
                    page.query().offset().saturating_sub(count)
                };
                let revision = page.query().revision();
                return Some(self.refresh_image_page(revision, offset));
            }
            _ => return None,
        }
        self.chat.workbench.scroll = 0;
        Some(Vec::new())
    }
}
