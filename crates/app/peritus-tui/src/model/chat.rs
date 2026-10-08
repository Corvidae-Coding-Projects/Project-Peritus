//! Persistent conversational composer, explicit slash commands, and provider catalog selection.

mod binding;
mod catalog;
mod commands;
mod doctor;
mod keys;
mod models;
mod navigation;
mod picker;
#[cfg(test)]
mod tests;
mod workbench;
mod working;

use super::{AppModel, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::{
    AppRequestPayload, ProductInteractionMode, ProductInteractionSnapshot, ProductModelCatalog,
    ProductRoleModels, ProductRunControlAction, ProductRunOperationState,
};
use peritus_types::RunId;

pub use working::WorkingIndicator;

#[derive(Debug)]
pub struct ChatUi {
    pub(crate) buffer: String,
    pub(crate) cursor: usize,
    pub(crate) selection_anchor: Option<usize>,
    pub(crate) viewport: Option<ratatui::layout::Rect>,
    mouse_anchor: Option<usize>,
    pub(crate) run_id: Option<RunId>,
    pub(in crate::model) binding_checked: Option<RunId>,
    pub(crate) snapshot: Option<ProductInteractionSnapshot>,
    pub(crate) mode: ProductInteractionMode,
    pub(crate) models: ProductRoleModels,
    pub(crate) scroll: usize,
    pub(crate) expanded: bool,
    pub(crate) output_mode: navigation::OutputMode,
    pub(crate) output_selection: Option<crate::input::output::OutputSelection>,
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
            selection_anchor: None,
            viewport: None,
            mouse_anchor: None,
            run_id: None,
            binding_checked: None,
            snapshot: None,
            mode: ProductInteractionMode::Chat,
            models: ProductRoleModels::default(),
            scroll: 0,
            expanded: false,
            output_mode: navigation::OutputMode::Live,
            output_selection: None,
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
    pub(crate) const fn selecting_output(&self) -> bool {
        matches!(self.output_mode, navigation::OutputMode::Selecting)
    }

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
        self.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.snapshot().operation().state() == ProductRunOperationState::Running
        })
    }
    pub(crate) fn matching_commands(&self) -> Vec<(String, &'static str)> {
        catalog::completions(&self.buffer)
    }
}
impl AppModel {
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
                "Disconnected; draft retained. Ctrl-R reconnects without replacing your message.",
            );
            return Vec::new();
        }
        if self.needs_chat_binding() {
            self.notice(
                NoticeLevel::Info,
                "Loading this run's conversation; draft retained. Send again after it opens.",
            );
            return self
                .chat
                .run_id
                .and_then(|run| self.query_chat_binding(run, true))
                .into_iter()
                .collect();
        }
        if self.chat.run_id.is_some() && self.chat.workbench.selected.is_none() {
            self.chat.run_id = None;
            self.chat.binding_checked = None;
            self.notice(
                NoticeLevel::Warning,
                "This run has no durable conversation destination and was detached. Press Enter again to start a new conversation; draft retained.",
            );
            return Vec::new();
        }
        self.send_workbench_chat(text)
    }
    pub(super) fn poll_chat(&mut self) -> Vec<Effect> {
        if self.pending.values().any(|pending| {
            matches!(
                pending,
                PendingRequest::ChatQuery
                    | PendingRequest::ChatOpen { .. }
                    | PendingRequest::ChatBinding { .. }
                    | PendingRequest::ProductMessageBinding { .. }
            )
        }) {
            return Vec::new();
        }
        let mut effects: Vec<_> = self
            .chat
            .run_id
            .and_then(|run_id| self.query_chat_binding(run_id, false))
            .into_iter()
            .collect();
        if self.chat.workbench.goal_mode
            && !self.workbench_request_pending()
            && let Some(goal) = self.chat.workbench.goal.as_ref()
            && Some(goal.query()) == self.chat.workbench.selected
            && Some(goal.run()) == self.chat.run_id
            && matches!(
                goal.state(),
                peritus_app_protocol::WorkbenchGoalState::Active
                    | peritus_app_protocol::WorkbenchGoalState::Pausing
            )
        {
            let query = goal.query();
            effects.extend(self.request(
                AppRequestPayload::QueryWorkbenchGoal(query),
                PendingRequest::WorkbenchGoal(query),
            ));
        }
        effects
    }
    pub(super) fn accept_chat(&mut self, snapshot: ProductInteractionSnapshot) {
        if self.chat.run_id != Some(snapshot.snapshot().run_id()) {
            return;
        }
        if self.product.as_ref().is_some_and(|product| {
            product.launch.workspace_id() != snapshot.snapshot().workspace_id()
        }) {
            self.notice(NoticeLevel::Error, "This conversation belongs to another workspace. Open its project before continuing.");
            return;
        }
        if self.chat.snapshot.is_none() {
            self.chat.mode = snapshot.mode();
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
    pub(super) fn chat_control(&mut self, action: ProductRunControlAction) -> Vec<Effect> {
        if action == ProductRunControlAction::Cancel {
            if let Some(effects) = self.stop_workbench_submission() {
                return effects;
            }
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
        self.chat.active() || self.workbench_chat_starting()
    }
    pub(crate) fn chat_control_is_legal(&self, action: ProductRunControlAction) -> bool {
        let Some(run_id) = self.chat.run_id else { return false };
        self.product
            .as_ref()
            .and_then(|product| product.runs.iter().find(|run| run.run_id() == run_id))
            .is_some_and(|run| run.operation().legal_controls().allows(action))
    }
    pub(super) fn chat_submission_pending(&self) -> bool {
        self.chat_mutation_pending()
            || self.pending.values().any(|pending| {
                matches!(
                    pending,
                    PendingRequest::ChatOpen { .. } | PendingRequest::ChatBinding { .. }
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
