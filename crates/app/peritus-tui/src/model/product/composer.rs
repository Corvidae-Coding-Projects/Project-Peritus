//! Task and follow-up modal composition with retained request drafts.

use super::ProductUi;
use crate::model::{AppModel, Editor, EditorKind, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::{
    AppRequestPayload, ProductRunContinuation, ProductRunRequest, ProductRunSnapshot,
};
use peritus_types::RunId;

impl AppModel {
    pub(in crate::model) fn open_task_composer(&mut self) {
        if self
            .product
            .as_ref()
            .is_some_and(|product| product.launch.direct_folder_writable().is_some())
        {
            self.notice(NoticeLevel::Info, "Use /chat to request in-place folder changes. Checked candidate delivery requires a managed Git workspace.");
            return;
        }
        if self.product.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Start Peritus through the `peritus` command to create coding runs",
            );
            return;
        }
        self.open_editor(Editor {
            kind: EditorKind::ProductTask,
            title: "New coding task",
            hint: "Describe the outcome. Shift-Enter adds a line; Enter starts the run.",
            buffer: String::new(),
            cursor: 0,
        });
    }

    pub(in crate::model) fn submit_product_task(&mut self, task: String) -> Vec<Effect> {
        let Some(run_id) = self.ids.run() else {
            self.notice(NoticeLevel::Error, "could not allocate a run identity");
            return Vec::new();
        };
        let Some(product) = &self.product else { return Vec::new() };
        let Some(providers) = product.providers() else {
            self.notice(
                NoticeLevel::Warning,
                "No provider is configured. Run `peritus providers` to sign in or add one.",
            );
            return Vec::new();
        };
        let request =
            match ProductRunRequest::new(run_id, product.launch.workspace_id(), providers, task) {
                Ok(request) => request,
                Err(error) => {
                    self.notice(NoticeLevel::Error, error.to_string());
                    return Vec::new();
                }
            };
        self.request(AppRequestPayload::StartProductRun(request), PendingRequest::ProductStart)
            .into_iter()
            .collect()
    }

    pub(in crate::model) fn open_product_message_composer(&mut self) {
        let Some(run_id) =
            self.product.as_ref().and_then(ProductUi::selected_run).map(ProductRunSnapshot::run_id)
        else {
            self.notice(NoticeLevel::Warning, "select a coding run before sending a message");
            return;
        };
        self.open_run_message_composer(run_id);
    }

    pub(in crate::model) fn open_run_message_composer(&mut self, run_id: RunId) {
        self.open_editor(Editor {
            kind: EditorKind::ProductMessage(run_id),
            title: "Message this coding run",
            hint: "Reply, redirect, add context, or say continue. Shift-Enter adds a line.",
            buffer: String::new(),
            cursor: 0,
        });
    }

    pub(in crate::model) fn submit_product_message(
        &mut self,
        run_id: RunId,
        message: String,
    ) -> Vec<Effect> {
        let continuation = match ProductRunContinuation::new(run_id, message) {
            Ok(continuation) => continuation,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        self.request(
            AppRequestPayload::ContinueProductRun(continuation),
            PendingRequest::ProductContinue,
        )
        .into_iter()
        .collect()
    }
}
