//! Persistent conversational composer, explicit slash commands, and provider catalog selection.

mod commands;
mod models;
#[cfg(test)]
mod tests;
mod working;

use super::{AppModel, Effect, NoticeLevel, PendingRequest};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    AppRequestPayload, ProductInteractionMode, ProductInteractionRequest,
    ProductInteractionSnapshot, ProductModelCatalog, ProductRoleModels, ProductRunControlAction,
    ProductRunConversationQuery, ProductRunRequest,
};
use peritus_types::RunId;

pub use commands::COMMANDS;
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
    pub(crate) model_picker: bool,
    pub(crate) model_selection: usize,
    pub(crate) model_role: models::ModelRole,
    interrupt_requested: bool,
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
            model_picker: false,
            model_selection: 0,
            model_role: models::ModelRole::Writer,
            interrupt_requested: false,
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
    pub(crate) fn matching_commands(&self) -> Vec<(&'static str, &'static str)> {
        if !self.buffer.starts_with('/') || self.buffer.contains(char::is_whitespace) {
            return Vec::new();
        }
        COMMANDS.iter().copied().filter(|(name, _)| name.starts_with(&self.buffer)).collect()
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
    pub(super) fn paste_chat(&mut self, text: &str) {
        self.chat.interrupt_requested = false;
        let text: String =
            text.chars().filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t').collect();
        if self.chat.buffer.len().saturating_add(text.len())
            > peritus_app_protocol::MAX_PRODUCT_TASK_BYTES
        {
            self.notice(NoticeLevel::Warning, "Message is too large; paste a smaller selection");
            return;
        }
        self.chat.buffer.insert_str(self.chat.cursor, &text);
        self.chat.cursor += text.len();
    }
    pub(super) fn handle_chat_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if !(key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')) {
            self.chat.interrupt_requested = false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') => {
                    self.quitting = true;
                    return vec![Effect::Quit];
                }
                KeyCode::Char('c') => {
                    if self.chat_work_active() && !self.chat.interrupt_requested {
                        self.chat.interrupt_requested = true;
                        let effects = self.chat_control(ProductRunControlAction::Cancel);
                        self.notice(NoticeLevel::Info, if effects.is_empty() {
                            "Stop could not be sent. Press Ctrl+C again to close; daemon work may continue."
                        } else {
                            "Stop requested. Press Ctrl+C again to close without waiting."
                        });
                        return effects;
                    }
                    self.quitting = true;
                    return vec![Effect::Quit];
                }
                _ => {}
            }
        }
        if self.chat.model_picker {
            return self.model_picker_key(key);
        }
        let commands = self.chat.matching_commands();
        match key.code {
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => self.paste_chat("\n"),
            KeyCode::Enter => return self.submit_chat(),
            KeyCode::Tab if !commands.is_empty() => {
                self.chat.buffer =
                    format!("{} ", commands[self.chat.command_selection.min(commands.len() - 1)].0);
                self.chat.cursor = self.chat.buffer.len();
                self.chat.command_selection = 0;
            }
            KeyCode::Up if !commands.is_empty() => {
                self.chat.command_selection = self.chat.command_selection.saturating_sub(1);
            }
            KeyCode::Down if !commands.is_empty() => {
                self.chat.command_selection =
                    (self.chat.command_selection + 1).min(commands.len() - 1);
            }
            KeyCode::PageUp => self.chat.scroll = self.chat.scroll.saturating_add(12),
            KeyCode::PageDown => self.chat.scroll = self.chat.scroll.saturating_sub(12),
            KeyCode::End if self.chat.buffer.is_empty() => self.chat.scroll = 0,
            KeyCode::Esc => {
                self.chat.expanded = false;
                self.chat.command_selection = 0;
            }
            _ => {
                if self.chat.buffer.len() < peritus_app_protocol::MAX_PRODUCT_TASK_BYTES
                    || !matches!(key.code, KeyCode::Char(_))
                {
                    let _ =
                        crate::input::edit_text(&mut self.chat.buffer, &mut self.chat.cursor, key);
                }
            }
        }
        Vec::new()
    }
    fn submit_chat(&mut self) -> Vec<Effect> {
        let text = self.chat.buffer.trim().to_owned();
        if text.is_empty() {
            return Vec::new();
        }
        if text.starts_with('/') {
            return self.slash_command(&text);
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
