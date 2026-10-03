//! Follow-up modal composition with retained request drafts.

use super::ProductUi;
use crate::model::{AppModel, Editor, EditorKind, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::{
    AppRequestPayload, ProductInteractionQuery, ProductRunSnapshot, WorkbenchInputText,
};
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
        if let Err(error) = WorkbenchInputText::new(message.clone()) {
            self.notice(NoticeLevel::Error, error.to_string());
            return Vec::new();
        }
        if self.context.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Disconnected; draft retained. Ctrl-R reconnects without replacing your message.",
            );
            return Vec::new();
        }
        self.abandon_chat_observations();
        if self.chat.run_id != Some(run_id) {
            self.select_workbench_conversation(None);
            self.chat.binding_checked = None;
        }
        self.chat.run_id = Some(run_id);
        self.chat.buffer.clone_from(&message);
        self.chat.snapshot = None;
        self.request(
            AppRequestPayload::QueryInteractionBinding(ProductInteractionQuery::new(run_id)),
            PendingRequest::ProductMessageBinding { run_id, message },
        )
        .into_iter()
        .collect()
    }
}
