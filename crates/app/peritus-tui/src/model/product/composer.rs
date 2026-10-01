//! Follow-up modal composition with retained request drafts.

use super::ProductUi;
use crate::model::{AppModel, Editor, EditorKind, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::{AppRequestPayload, ProductRunContinuation, ProductRunSnapshot};
use peritus_types::RunId;

impl AppModel {
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
            hint: "Reply, redirect, add context, or say continue. Type /runs to return to the dashboard.",
            buffer: String::new(),
            cursor: 0,
            pasted_command: false,
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
