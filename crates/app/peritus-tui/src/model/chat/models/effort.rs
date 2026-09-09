//! Discoverable effort picker and exact per-role command handling.

use super::{AppModel, Effect, KeyCode, KeyEvent, ModelRole, NoticeLevel};
use peritus_app_protocol::ProductModelEffort;

impl AppModel {
    pub(in crate::model::chat) fn effort_command(&mut self, arguments: &str) -> Vec<Effect> {
        let mut words = arguments.split_whitespace();
        let first = words.next();
        let value = match first {
            Some("writer") => {
                self.chat.model_role = ModelRole::Writer;
                words.next()
            }
            Some("reviewer") => {
                self.chat.model_role = ModelRole::Reviewer;
                words.next()
            }
            Some("fixer") => {
                self.chat.model_role = ModelRole::Fixer;
                words.next()
            }
            other => {
                self.chat.model_role =
                    if self.chat.mode == peritus_app_protocol::ProductInteractionMode::Review {
                        ModelRole::Reviewer
                    } else {
                        ModelRole::Writer
                    };
                other
            }
        };
        if words.next().is_some() {
            return self.invalid_effort();
        }
        let Some(value) = value else {
            self.clear_chat_command();
            self.open_effort_picker();
            return Vec::new();
        };
        let Some(effort) = ProductModelEffort::parse(value) else {
            return self.invalid_effort();
        };
        let choice = self.chat.model_role.choice(&self.chat.models).clone().with_effort(effort);
        self.save_model_choice(choice)
    }

    pub(super) fn open_effort_picker(&mut self) {
        let current = self.chat.model_role.choice(&self.chat.models).effort();
        self.chat.effort_selection =
            ProductModelEffort::ALL.iter().position(|effort| *effort == current).unwrap_or(0);
        self.chat.close_model_picker();
        self.chat.show_effort_picker();
    }

    pub(in crate::model::chat) fn effort_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => self.chat.close_effort_picker(),
            KeyCode::Tab => {
                self.chat.model_role = self.chat.model_role.next();
                self.open_effort_picker();
            }
            KeyCode::Up => {
                self.chat.effort_selection = self.chat.effort_selection.saturating_sub(1);
            }
            KeyCode::Down => {
                self.chat.effort_selection =
                    (self.chat.effort_selection + 1).min(ProductModelEffort::ALL.len() - 1);
            }
            KeyCode::Home => self.chat.effort_selection = 0,
            KeyCode::End => self.chat.effort_selection = ProductModelEffort::ALL.len() - 1,
            KeyCode::Enter => {
                if let Some(effort) = ProductModelEffort::ALL.get(self.chat.effort_selection) {
                    let choice =
                        self.chat.model_role.choice(&self.chat.models).clone().with_effort(*effort);
                    return self.save_model_choice(choice);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn invalid_effort(&mut self) -> Vec<Effect> {
        self.notice(
            NoticeLevel::Warning,
            "Usage: /effort [writer|reviewer|fixer] [default|minimal|low|medium|high|xhigh|max|ultra]",
        );
        Vec::new()
    }
}
