//! Explicit provider replacement for future turns, preserving the current run and its inputs.
use super::{AppModel, Effect, NoticeLevel, PendingRequest, models::ModelRole};
use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{
    AppRequestPayload, ProductModelChoice, ProductModelUpdate, ProductProviderSelection,
    ProductRoleModels, WellKnownProtocolFeature,
};

impl AppModel {
    pub(super) fn provider_command(&mut self, arguments: &str) -> Vec<Effect> {
        let mut words = arguments.split_whitespace();
        let first = words.next();
        let (role, selection) = match first {
            Some("writer") => (Some(ModelRole::Writer), words.next()),
            Some("reviewer") => (Some(ModelRole::Reviewer), words.next()),
            Some("fixer") => (Some(ModelRole::Fixer), words.next()),
            Some("all") => (None, words.next()),
            other => (None, other),
        };
        if words.next().is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Usage: /provider [all|writer|reviewer|fixer] [NUMBER]",
            );
            return Vec::new();
        }
        self.chat.provider_role = role;
        if let Some(number) = selection {
            if let Some(index) =
                number.parse::<usize>().ok().and_then(|number| number.checked_sub(1))
            {
                return self.choose_chat_provider(index);
            }
            self.notice(
                NoticeLevel::Warning,
                "Choose a displayed provider number with /provider NUMBER.",
            );
            return Vec::new();
        }
        self.clear_chat_command();
        self.chat.model_selection = 0;
        self.chat.show_provider_picker();
        Vec::new()
    }

    pub(super) fn provider_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let count = self.product.as_ref().map_or(0, |product| product.launch.providers().len());
        match key.code {
            KeyCode::Esc => self.chat.close_provider_picker(),
            KeyCode::Up => self.chat.model_selection = self.chat.model_selection.saturating_sub(1),
            KeyCode::Down => {
                self.chat.model_selection =
                    self.chat.model_selection.saturating_add(1).min(count.saturating_sub(1));
            }
            KeyCode::Home => self.chat.model_selection = 0,
            KeyCode::End => self.chat.model_selection = count.saturating_sub(1),
            KeyCode::Tab => {
                self.chat.provider_role = match self.chat.provider_role {
                    None => Some(ModelRole::Writer),
                    Some(ModelRole::Writer) => Some(ModelRole::Reviewer),
                    Some(ModelRole::Reviewer) => Some(ModelRole::Fixer),
                    Some(ModelRole::Fixer) => None,
                }
            }
            KeyCode::Enter => return self.choose_chat_provider(self.chat.model_selection),
            _ => {}
        }
        Vec::new()
    }

    fn choose_chat_provider(&mut self, index: usize) -> Vec<Effect> {
        if self.chat_submission_pending() {
            self.notice(
                NoticeLevel::Warning,
                "Wait for the pending conversation update before changing providers.",
            );
            return Vec::new();
        }
        let Some(selected) = self
            .product
            .as_ref()
            .and_then(|product| product.launch.providers().get(index))
            .cloned()
        else {
            self.notice(
                NoticeLevel::Warning,
                "No provider at that number. Configure providers with `peritus providers` first.",
            );
            return Vec::new();
        };
        let Some(current) = self.chat_providers() else { return Vec::new() };
        let id = selected.profile_id();
        let role = self.chat.provider_role;
        let writer =
            if role.is_none() || role == Some(ModelRole::Writer) { id } else { current.writer() };
        let reviewer = if role.is_none() || role == Some(ModelRole::Reviewer) {
            id
        } else {
            current.reviewer()
        };
        let fixer =
            if role.is_none() || role == Some(ModelRole::Fixer) { id } else { current.fixer() };
        let providers = ProductProviderSelection::new(writer, reviewer, fixer);
        let prior = &self.chat.models;
        let models = ProductRoleModels::new(
            if writer == current.writer() {
                prior.writer().clone()
            } else {
                ProductModelChoice::default()
            },
            if reviewer == current.reviewer() {
                prior.reviewer().clone()
            } else {
                ProductModelChoice::default()
            },
            if fixer == current.fixer() {
                prior.fixer().clone()
            } else {
                ProductModelChoice::default()
            },
        );
        self.clear_chat_command();
        self.abandon_model_discovery();
        self.chat.catalog = None;
        self.chat.close_provider_picker();
        if let Some(run_id) = self.chat.run_id {
            if !self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::ProviderSelection.as_str()
            }) {
                self.notice(NoticeLevel::Warning, "This daemon does not support provider changes on retained runs. Update the daemon, then reconnect; the run is retained.");
                return Vec::new();
            }
            return self
                .request(
                    AppRequestPayload::UpdateModels(
                        ProductModelUpdate::new(run_id, models).with_providers(providers),
                    ),
                    PendingRequest::ModelUpdate { run_id },
                )
                .into_iter()
                .collect();
        }
        if writer != current.writer() {
            self.chat.workbench.files.discard_preview();
            self.chat.workbench.images.discard_preview();
        }
        if let Some(product) = &mut self.product {
            product.select_providers(providers);
        }
        self.chat.models = models;
        self.notice(NoticeLevel::Info, format!("Provider selected: {}. Changed roles use their configured model; /model selects another.", selected.label()));
        Vec::new()
    }
}
