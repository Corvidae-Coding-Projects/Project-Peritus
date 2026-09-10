//! Persistent conversational composer, explicit slash commands, and provider catalog selection.

mod catalog;
mod commands;
mod doctor;
mod keys;
mod models;
mod picker;
#[cfg(test)]
mod tests;
mod workbench;
mod working;

use super::{AppModel, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::{
    AppRequestPayload, ProductInteractionMode, ProductInteractionRequest,
    ProductInteractionSnapshot, ProductModelCatalog, ProductRoleModels, ProductRunControlAction,
    ProductRunConversationQuery, ProductRunRequest,
};
use peritus_types::RunId;

pub use working::WorkingIndicator;

#[derive(Debug)]
pub struct ChatUi {
    pub(crate) buffer: String,
    pub(crate) cursor: usize,
    pub(crate) run_id: Option<RunId>,
    pub(crate) snapshot: Option<ProductInteractionSnapshot>,
    pub(crate) mode: ProductInteractionMode,
    pub(crate) models: ProductRoleModels,
    pub(crate) scroll: usize,
    pub(crate) expanded: bool,
    pub(crate) working: WorkingIndicator,
    pub(crate) command_selection: usize,
    pub(crate) catalog: Option<ProductModelCatalog>,
    picker: Option<picker::Picker>,
    pub(crate) effort_selection: usize,
    pub(crate) model_selection: usize,
    pub(crate) model_role: models::ModelRole,
    pub(crate) doctor: Option<doctor::DoctorPanel>,
    pub(crate) workbench: workbench::WorkbenchUi,
    interrupt_requested: bool,
    pasted_command: bool,
}
impl Default for ChatUi {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            run_id: None,
            snapshot: None,
            mode: ProductInteractionMode::Chat,
            models: ProductRoleModels::default(),
            scroll: 0,
            expanded: false,
            working: WorkingIndicator::default(),
            command_selection: 0,
            catalog: None,
            picker: None,
            effort_selection: 0,
            model_selection: 0,
            model_role: models::ModelRole::Writer,
            doctor: None,
            workbench: workbench::WorkbenchUi::default(),
            interrupt_requested: false,
            pasted_command: false,
        }
    }
}
impl ChatUi {
    pub(super) fn status(&self) -> String {
        self.snapshot.as_ref().map_or_else(
            || "New conversation".to_owned(),
            |snapshot| {
                format!(
                    "{} · received {} · incorporated {}",
                    snapshot.snapshot().status(),
                    snapshot.received(),
                    snapshot.incorporated()
                )
            },
        )
    }
    pub(crate) fn active(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| !snapshot.snapshot().phase().terminal())
    }
    pub(crate) fn matching_commands(&self) -> Vec<(String, &'static str)> {
        catalog::completions(&self.buffer)
    }
}
impl AppModel {
    pub(super) fn open_selected_conversation(&mut self) -> Vec<Effect> {
        let Some(run_id) = self
            .product
            .as_ref()
            .and_then(|product| product.selected_run())
            .map(peritus_app_protocol::ProductRunSnapshot::run_id)
        else {
            return Vec::new();
        };
        self.chat.run_id = Some(run_id);
        self.chat.snapshot = None;
        self.request(
            AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run_id)),
            PendingRequest::ChatOpen { run_id },
        )
        .into_iter()
        .collect()
    }
    fn submit_chat(&mut self) -> Vec<Effect> {
        let text = self.chat.buffer.trim().to_owned();
        if text.is_empty() {
            return Vec::new();
        }
        if text.starts_with('/') {
            if self.chat.pasted_command {
                self.notice(NoticeLevel::Warning,
                    "Pasted commands do not execute. Type the command or select it with Tab; draft retained.");
                return Vec::new();
            }
            return self.slash_command(&text);
        }
        if let Some(path) = text.strip_prefix('@') {
            return self.file_command(path);
        }
        self.send_chat_message(text)
    }
    pub(super) fn send_chat_message(&mut self, text: String) -> Vec<Effect> {
        if self.chat_submission_pending() {
            self.notice(
                NoticeLevel::Info,
                "Waiting for the previous input receipt; this draft is retained.",
            );
            return Vec::new();
        }
        if self.context.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Disconnected; your draft is retained. Use /reconnect.",
            );
            return Vec::new();
        }
        let Some(product) = &self.product else {
            return Vec::new();
        };
        let Some(providers) = self.chat_providers() else {
            self.notice(
                NoticeLevel::Warning,
                "No provider configured. Run peritus providers first.",
            );
            return Vec::new();
        };
        let workspace = product.launch.workspace_id();
        let Some(run_id) = self.chat.run_id.or_else(|| self.ids.run()) else {
            return Vec::new();
        };
        let request = match ProductRunRequest::new(run_id, workspace, providers, text.clone()) {
            Ok(request) => request,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        let effect = self.request(
            AppRequestPayload::Interact(ProductInteractionRequest::new(
                request,
                self.chat.mode,
                self.chat.models.clone(),
            )),
            PendingRequest::ChatSubmit { run_id, text },
        );
        if effect.is_some() {
            self.chat.run_id = Some(run_id);
            self.chat.buffer.clear();
            self.chat.cursor = 0;
            self.chat.scroll = 0;
            self.notice(NoticeLevel::Info, "Sending input; durable receipt not yet confirmed");
        }
        effect.into_iter().collect()
    }
    pub(super) fn poll_chat(&mut self) -> Vec<Effect> {
        if self.pending.values().any(|pending| {
            matches!(pending, PendingRequest::ChatQuery | PendingRequest::ChatSubmit { .. })
        }) {
            return Vec::new();
        }
        self.chat
            .run_id
            .and_then(|run_id| {
                self.request(
                    AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run_id)),
                    PendingRequest::ChatQuery,
                )
            })
            .into_iter()
            .collect()
    }
    pub(super) fn accept_chat(&mut self, snapshot: ProductInteractionSnapshot) {
        if self.chat.run_id != Some(snapshot.snapshot().run_id()) {
            return;
        }
        // Model updates append a durable activity. A delayed pre-selection poll must not
        // overwrite a newer acknowledgement, including after a reconnect.
        if self.chat.snapshot.as_ref().is_some_and(|current| {
            snapshot.activities().last().map_or(0, peritus_app_protocol::ProductActivity::sequence)
                < current
                    .activities()
                    .last()
                    .map_or(0, peritus_app_protocol::ProductActivity::sequence)
        }) {
            return;
        }
        self.chat.models = snapshot.models().clone();
        self.accept_product_run(snapshot.snapshot().clone());
        if let Some(settlement) = snapshot.settlement()
            && let Some(product) = &mut self.product
        {
            product.settlements.insert(snapshot.snapshot().run_id(), *settlement);
        }
        self.chat.snapshot = Some(snapshot);
    }
    pub(super) fn restore_chat_draft(&mut self, text: &str) {
        self.chat.buffer = if self.chat.buffer.is_empty() {
            text.to_owned()
        } else {
            format!("{text}\n{}", self.chat.buffer)
        };
        self.chat.cursor = self.chat.buffer.len();
    }
    pub(super) fn recover_chat_drafts(&mut self) {
        let messages = self
            .pending
            .values()
            .filter_map(|pending| match pending {
                PendingRequest::ChatSubmit { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for text in messages {
            self.restore_chat_draft(&text);
        }
    }
    pub(super) fn chat_control(&mut self, action: ProductRunControlAction) -> Vec<Effect> {
        if action == ProductRunControlAction::Cancel {
            if !self.chat_work_active() {
                self.notice(NoticeLevel::Info, "No active work to stop.");
                return Vec::new();
            }
            return self
                .chat
                .run_id
                .and_then(|run_id| {
                    self.request(
                        AppRequestPayload::ControlProductRun(
                            peritus_app_protocol::ProductRunControl::new(run_id, action),
                        ),
                        PendingRequest::ProductControl,
                    )
                })
                .into_iter()
                .collect();
        }
        if !self.select_chat_run() {
            return Vec::new();
        }
        self.control_selected_product_run(action)
    }
    pub(super) fn chat_work_active(&self) -> bool {
        self.chat.active() || self.pending.values().any(|pending| {
            matches!(pending, PendingRequest::ChatSubmit { run_id, .. } if Some(*run_id) == self.chat.run_id)
        })
    }
    pub(super) fn chat_submission_pending(&self) -> bool {
        self.pending.values().any(|pending| {
            matches!(
                pending,
                PendingRequest::ChatSubmit { .. } | PendingRequest::ModelUpdate { .. }
            )
        })
    }
    pub(crate) fn chat_providers(&self) -> Option<peritus_app_protocol::ProductProviderSelection> {
        self.chat
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.snapshot().providers())
            .or_else(|| self.product.as_ref().and_then(super::product::ProductUi::providers))
    }
    pub(super) fn select_chat_run(&mut self) -> bool {
        if let Some(run_id) = self.chat.run_id
            && let Some(product) = &mut self.product
            && let Some(index) = product.runs.iter().position(|run| run.run_id() == run_id)
        {
            product.selected = index;
            return true;
        }
        self.notice(
            NoticeLevel::Warning,
            "This conversation has no observed run to inspect or control yet.",
        );
        false
    }
}
