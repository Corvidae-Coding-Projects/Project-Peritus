//! Thin UI adapter: dispatch the selection owner's query/save actions to the daemon.
use super::menu::{MenuAction, MenuStart, ModelSetup, ModelStep};
use super::{AppModel, Effect, KeyCode, KeyEvent, NoticeLevel};
use peritus_app_protocol::ProductModelChoice;

impl AppModel {
    pub(crate) fn model_setup_step(&self) -> Option<ModelStep> {
        self.chat.model_setup.as_ref().map(ModelSetup::step)
    }
    pub(crate) fn model_setup_choice(&self) -> Option<&ProductModelChoice> {
        self.chat.model_setup.as_ref().and_then(ModelSetup::choice)
    }
    pub(crate) fn model_picker_selection(&self) -> usize {
        self.chat.model_setup.as_ref().map_or(self.chat.model_selection, ModelSetup::selection)
    }
    pub(crate) fn effort_picker_selection(&self) -> usize {
        self.chat.model_setup.as_ref().map_or(self.chat.effort_selection, ModelSetup::selection)
    }

    pub(super) fn start_model_setup(&mut self, start: MenuStart) -> Vec<Effect> {
        let Some((menu, action)) = ModelSetup::start(self, start) else {
            self.notice(
                NoticeLevel::Warning,
                "Open a configured project before selecting agent models.",
            );
            return Vec::new();
        };
        self.clear_chat_command();
        self.chat.model_setup = Some(menu);
        self.apply_model_menu_action(action)
    }
    pub(in crate::model) fn cancel_model_setup(&mut self) {
        self.chat.model_setup = None;
        self.abandon_model_discovery();
        self.chat.close_model_picker();
        self.chat.close_effort_picker();
    }
    fn model_setup_is_current(&mut self) -> bool {
        let current = self.chat.model_setup.as_ref().is_some_and(|menu| menu.is_current(self));
        if !current {
            self.cancel_model_setup();
            self.notice(NoticeLevel::Warning,"The conversation, provider, or draft changed. Reopen /model to select settings for the current agent.");
        }
        current
    }
    pub(in crate::model::chat) fn model_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if self.chat.model_setup.is_none() {
            if key.code == KeyCode::Esc {
                self.cancel_model_setup();
            }
            return Vec::new();
        }
        if !self.model_setup_is_current() {
            return Vec::new();
        }
        if matches!(key.code, KeyCode::Esc | KeyCode::Backspace) {
            self.abandon_model_discovery();
        }
        let Some(menu) = &mut self.chat.model_setup else { return Vec::new() };
        let action = menu.key(key, &self.chat.models, self.chat.catalog.as_ref());
        self.apply_model_menu_action(action)
    }
    fn apply_model_menu_action(&mut self, action: MenuAction) -> Vec<Effect> {
        if let Some(menu) = &self.chat.model_setup {
            self.chat.model_role = menu.role();
            if menu.step() == ModelStep::Effort {
                self.chat.show_effort_picker();
            } else {
                self.chat.show_model_picker();
            }
        }
        match action {
            MenuAction::Discover => self.query_chat_models(false),
            MenuAction::Refresh => self.query_chat_models(true),
            MenuAction::Save(role, choice) => self.save_model_choice(role, choice),
            MenuAction::Cancel => {
                self.cancel_model_setup();
                Vec::new()
            }
            MenuAction::CatalogChanged => {
                self.notice(
                    NoticeLevel::Warning,
                    "The model catalog changed. Select the model again before saving.",
                );
                Vec::new()
            }
            MenuAction::Warning(message) => {
                self.notice(NoticeLevel::Warning, message);
                Vec::new()
            }
            MenuAction::None => Vec::new(),
        }
    }
}
