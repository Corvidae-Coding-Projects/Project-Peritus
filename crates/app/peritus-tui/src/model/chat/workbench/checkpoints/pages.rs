//! Lazy checkpoint coverage navigation.

use super::super::{AppModel, Effect, PendingRequest};
use peritus_app_protocol::{
    AppRequestPayload, WorkbenchCheckpointPageRequest, WorkbenchRewindPageRequest,
};

impl AppModel {
    pub(in crate::model::chat) fn next_checkpoint_page(&mut self) -> Vec<Effect> {
        if self.workbench_request_pending() {
            return Vec::new();
        }
        let (Some(page), Some(current)) = (
            self.chat.workbench.checkpoint_page.as_ref(),
            self.chat.workbench.checkpoint_page_request,
        ) else {
            return Vec::new();
        };
        let Some(cursor) = page.next() else { return Vec::new() };
        let Ok(request) = WorkbenchCheckpointPageRequest::new(
            current.query(),
            current.revision(),
            current.checkpoint(),
            Some(cursor),
        ) else {
            return Vec::new();
        };
        self.chat.workbench.checkpoint_page_history.push(Some(cursor));
        self.chat.workbench.checkpoint_page_request = Some(request);
        self.request(
            AppRequestPayload::QueryWorkbenchCheckpointPage(request),
            PendingRequest::WorkbenchCheckpointPage(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model::chat) fn previous_checkpoint_page(&mut self) -> Vec<Effect> {
        if self.workbench_request_pending() || self.chat.workbench.checkpoint_page_history.len() < 2
        {
            return Vec::new();
        }
        self.chat.workbench.checkpoint_page_history.pop();
        let Some(current) = self.chat.workbench.checkpoint_page_request else { return Vec::new() };
        let cursor = self.chat.workbench.checkpoint_page_history.last().copied().flatten();
        let Ok(request) = WorkbenchCheckpointPageRequest::new(
            current.query(),
            current.revision(),
            current.checkpoint(),
            cursor,
        ) else {
            return Vec::new();
        };
        self.chat.workbench.checkpoint_page_request = Some(request);
        self.request(
            AppRequestPayload::QueryWorkbenchCheckpointPage(request),
            PendingRequest::WorkbenchCheckpointPage(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model::chat) fn next_rewind_page(&mut self) -> Vec<Effect> {
        if self.workbench_request_pending() {
            return Vec::new();
        }
        let (Some(page), Some(current)) =
            (self.chat.workbench.rewind_page.as_ref(), self.chat.workbench.rewind_page_request)
        else {
            return Vec::new();
        };
        let Some(cursor) = page.next() else { return Vec::new() };
        let request = WorkbenchRewindPageRequest::new(current.request(), Some(cursor));
        self.chat.workbench.rewind_page_history.push(Some(cursor));
        self.chat.workbench.rewind_page_request = Some(request);
        self.request(
            AppRequestPayload::QueryWorkbenchRewindPage(request),
            PendingRequest::WorkbenchRewindPage(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model::chat) fn previous_rewind_page(&mut self) -> Vec<Effect> {
        if self.workbench_request_pending() || self.chat.workbench.rewind_page_history.len() < 2 {
            return Vec::new();
        }
        self.chat.workbench.rewind_page_history.pop();
        let Some(current) = self.chat.workbench.rewind_page_request else { return Vec::new() };
        let cursor = self.chat.workbench.rewind_page_history.last().copied().flatten();
        let request = WorkbenchRewindPageRequest::new(current.request(), cursor);
        self.chat.workbench.rewind_page_request = Some(request);
        self.request(
            AppRequestPayload::QueryWorkbenchRewindPage(request),
            PendingRequest::WorkbenchRewindPage(request),
        )
        .into_iter()
        .collect()
    }
}
