//! Agent -> model -> effort, with one destination-bound save after the last choice.
use super::{AppModel, Effect, KeyCode, KeyEvent, ModelRole, NoticeLevel};
use peritus_app_protocol::{
    ProductModelChoice, ProductModelEffort, ProductProviderSelection, WorkbenchQuery,
};
use peritus_types::RunId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelStep {
    Agent,
    Model,
    Effort,
}

#[derive(Debug)]
pub(in crate::model::chat) struct ModelSetup {
    run: Option<RunId>,
    conversation: Option<WorkbenchQuery>,
    connection: u64,
    providers: ProductProviderSelection,
    role: ModelRole,
    step: ModelStep,
    choice: Option<ProductModelChoice>,
}

impl AppModel {
    pub(crate) fn model_setup_step(&self) -> Option<ModelStep> {
        self.chat.model_setup.as_ref().map(|setup| setup.step)
    }

    pub(crate) fn model_setup_choice(&self) -> Option<&ProductModelChoice> {
        self.chat.model_setup.as_ref().and_then(|setup| setup.choice.as_ref())
    }

    pub(super) fn start_model_setup(&mut self, skip_agent: bool, refresh: bool) -> Vec<Effect> {
        let Some(providers) = self.chat_providers() else {
            self.notice(
                NoticeLevel::Warning,
                "Open a configured project before selecting agent models.",
            );
            return Vec::new();
        };
        self.clear_chat_command();
        self.chat.model_setup = Some(ModelSetup {
            run: self.chat.run_id,
            conversation: self.chat.workbench.selected,
            connection: self.connection_generation,
            providers,
            role: self.chat.model_role,
            step: ModelStep::Agent,
            choice: None,
        });
        self.chat.show_model_picker();
        if skip_agent { self.select_model_agent(refresh) } else { Vec::new() }
    }

    pub(in crate::model) fn cancel_model_setup(&mut self) {
        self.chat.model_setup = None;
        self.abandon_model_discovery();
        self.chat.close_model_picker();
        self.chat.close_effort_picker();
    }

    pub(super) fn model_setup_is_current(&mut self) -> bool {
        let valid = self.chat.model_setup.as_ref().is_some_and(|setup| {
            setup.run == self.chat.run_id
                && setup.conversation == self.chat.workbench.selected
                && setup.connection == self.connection_generation
                && Some(setup.providers) == self.chat_providers()
                && (setup.step == ModelStep::Agent || setup.role == self.chat.model_role)
                && self.chat.buffer.is_empty()
        });
        if !valid {
            self.cancel_model_setup();
            self.notice(NoticeLevel::Warning,"The conversation, provider, or draft changed. Reopen /model to select settings for the current agent.");
        }
        valid
    }

    fn select_model_agent(&mut self, refresh: bool) -> Vec<Effect> {
        if !self.model_setup_is_current() {
            return Vec::new();
        }
        if let Some(setup) = &mut self.chat.model_setup {
            setup.step = ModelStep::Model;
            setup.role = self.chat.model_role;
        }
        self.chat.model_selection = self
            .chat
            .catalog
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .models()
                    .iter()
                    .position(|m| m.id() == self.chat.model_role.choice(&self.chat.models).id())
            })
            .unwrap_or(0);
        self.query_chat_models(refresh)
    }

    pub(super) fn model_setup_back(&mut self) {
        self.abandon_model_discovery();
        match self.model_setup_step() {
            Some(ModelStep::Effort) => {
                if let Some(setup) = &mut self.chat.model_setup {
                    setup.step = ModelStep::Model;
                    setup.choice = None;
                }
                self.chat.show_model_picker();
            }
            Some(ModelStep::Model) => {
                if let Some(setup) = &mut self.chat.model_setup {
                    setup.step = ModelStep::Agent;
                }
                self.chat.show_model_picker();
            }
            _ => self.cancel_model_setup(),
        }
    }

    fn select_setup_model(&mut self) {
        let Some(entry) = self
            .chat
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.models().get(self.chat.model_selection))
        else {
            return;
        };
        let choice = match ProductModelChoice::new(entry.id().to_owned(), false) {
            Ok(choice) => {
                choice.with_effort(self.chat.model_role.choice(&self.chat.models).effort())
            }
            Err(error) => {
                self.notice(NoticeLevel::Warning, error.to_string());
                return;
            }
        };
        self.chat.effort_selection =
            ProductModelEffort::ALL.iter().position(|e| *e == choice.effort()).unwrap_or(0);
        if let Some(setup) = &mut self.chat.model_setup {
            setup.choice = Some(choice);
            setup.step = ModelStep::Effort;
        }
        self.chat.show_effort_picker();
    }

    pub(super) fn save_model_setup(&mut self, effort: ProductModelEffort) -> Vec<Effect> {
        if !self.model_setup_is_current() {
            return Vec::new();
        }
        let Some(choice) = self.model_setup_choice().cloned() else { return Vec::new() };
        if !self.chat.catalog.as_ref().is_some_and(|catalog| {
            Some(catalog.profile()) == self.role_profile()
                && catalog.models().iter().any(|m| m.id() == choice.id())
        }) {
            self.model_setup_back();
            self.notice(
                NoticeLevel::Warning,
                "The model catalog changed. Select the model again before saving.",
            );
            return Vec::new();
        }
        self.save_model_choice(choice.with_effort(effort))
    }

    pub(in crate::model::chat) fn model_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if self.model_setup_step().is_none() {
            if key.code == KeyCode::Esc {
                self.cancel_model_setup();
            }
            return Vec::new();
        }
        if !self.model_setup_is_current() {
            return Vec::new();
        }
        if key.code == KeyCode::Esc {
            self.model_setup_back();
            return Vec::new();
        }
        if self.model_setup_step() == Some(ModelStep::Agent) {
            return self.agent_picker_key(key);
        }
        match key.code {
            KeyCode::Char('r') => return self.query_chat_models(true),
            KeyCode::Backspace => self.model_setup_back(),
            KeyCode::Up => self.chat.model_selection = self.chat.model_selection.saturating_sub(1),
            KeyCode::Down => {
                self.chat.model_selection = (self.chat.model_selection + 1).min(
                    self.chat.catalog.as_ref().map_or(0, |c| c.models().len().saturating_sub(1)),
                );
            }
            KeyCode::Home => self.chat.model_selection = 0,
            KeyCode::End => {
                self.chat.model_selection =
                    self.chat.catalog.as_ref().map_or(0, |c| c.models().len().saturating_sub(1));
            }
            KeyCode::Enter | KeyCode::Char('e') => self.select_setup_model(),
            _ => {}
        }
        Vec::new()
    }

    fn agent_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Char('1' | '2' | '3') => {
                self.chat.model_role = match key.code {
                    KeyCode::Char('1') => ModelRole::Writer,
                    KeyCode::Char('2') => ModelRole::Reviewer,
                    _ => ModelRole::Fixer,
                };
                return self.select_model_agent(false);
            }
            KeyCode::Enter => return self.select_model_agent(false),
            KeyCode::Up => self.chat.model_role = self.chat.model_role.next().next(),
            KeyCode::Down | KeyCode::Tab => self.chat.model_role = self.chat.model_role.next(),
            KeyCode::Home => self.chat.model_role = ModelRole::Writer,
            KeyCode::End => self.chat.model_role = ModelRole::Fixer,
            _ => {}
        }
        Vec::new()
    }
}
