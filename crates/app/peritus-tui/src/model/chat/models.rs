//! Exact provider-discovered selection; manual identifiers are always explicitly marked.

use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest};
use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{
    AppRequestPayload, ProductModelCatalog, ProductModelChoice, ProductModelQuery,
    ProductRoleModels,
};

mod effort;

mod direct;
pub(super) use direct::PendingModelSelection;
mod destination;
mod menu;
mod wizard;
use menu::MenuStart;
pub(super) use menu::ModelSetup;
pub use menu::ModelStep;

#[cfg(test)]
#[path = "../../../../../../discovery/proptest-praxis/owner/tui_role_models.rs"]
mod replay;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelRole {
    Writer,
    Reviewer,
    Fixer,
}
impl ModelRole {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Writer => "chat/writer",
            Self::Reviewer => "reviewer",
            Self::Fixer => "fixer",
        }
    }
    pub(super) const fn next(self) -> Self {
        match self {
            Self::Writer => Self::Reviewer,
            Self::Reviewer => Self::Fixer,
            Self::Fixer => Self::Writer,
        }
    }

    pub(crate) const fn choice(self, models: &ProductRoleModels) -> &ProductModelChoice {
        match self {
            Self::Writer => models.writer(),
            Self::Reviewer => models.reviewer(),
            Self::Fixer => models.fixer(),
        }
    }
}

impl AppModel {
    pub(super) fn model_command(&mut self, arguments: &str) -> Vec<Effect> {
        self.cancel_model_setup();
        if arguments.trim().is_empty() {
            return self.start_model_setup(MenuStart::Agents);
        }
        let mut words = arguments.split_whitespace();
        let first = words.next();
        let command = match first {
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
            other => other,
        };
        match command {
            Some("manual") => self.direct_model_words(words, true),
            None | Some("refresh") => {
                if words.next().is_some() {
                    return self.model_usage();
                }
                self.start_model_setup(if command == Some("refresh") {
                    MenuStart::CatalogRefresh
                } else {
                    MenuStart::Models
                })
            }
            Some(id) => self.direct_model_words(std::iter::once(id).chain(words), false),
        }
    }

    fn query_chat_models(&mut self, refresh: bool) -> Vec<Effect> {
        let Some(providers) = self.chat_providers() else {
            return Vec::new();
        };
        let profile = match self.chat.model_role {
            ModelRole::Writer => providers.writer(),
            ModelRole::Reviewer => providers.reviewer(),
            ModelRole::Fixer => providers.fixer(),
        };
        if self.pending.values().any(|pending| {
            matches!(pending, PendingRequest::ModelQuery(query) if query.profile() == profile)
        }) {
            self.notice(NoticeLevel::Info, "Model discovery is already pending; Esc returns.");
            return Vec::new();
        }
        self.abandon_model_discovery();
        if self.chat.catalog.as_ref().is_some_and(|catalog| catalog.profile() != profile) {
            self.chat.catalog = None;
        }
        let query = ProductModelQuery::new(profile, refresh);
        self.request(AppRequestPayload::QueryModels(query), PendingRequest::ModelQuery(query))
            .into_iter()
            .collect()
    }

    pub(in crate::model) fn abandon_model_discovery(&mut self) {
        self.chat.pending_model = None;
        self.pending.retain(|_, pending| !matches!(pending, PendingRequest::ModelQuery(_)));
        self.pending_started.retain(|id, _| self.pending.contains_key(id));
    }

    pub(crate) fn model_discovery_pending(&self) -> bool {
        self.pending.values().any(|pending| matches!(pending, PendingRequest::ModelQuery(_)))
    }

    pub(in crate::model) fn accept_model_response(
        &mut self,
        catalog: &ProductModelCatalog,
        pending: Option<&PendingRequest>,
    ) -> Vec<Effect> {
        if matches!(pending, Some(PendingRequest::ModelQuery(query)) if query.profile() == catalog.profile())
        {
            self.accept_model_catalog(catalog.clone());
            return self.finish_direct_model_selection();
        }
        Vec::new()
    }

    pub(in crate::model) fn accept_model_catalog(&mut self, catalog: ProductModelCatalog) {
        let profile = self.chat_providers().map(|providers| match self.chat.model_role {
            ModelRole::Writer => providers.writer(),
            ModelRole::Reviewer => providers.reviewer(),
            ModelRole::Fixer => providers.fixer(),
        });
        if profile != Some(catalog.profile()) {
            return;
        }
        if let Some(menu) = &mut self.chat.model_setup {
            menu.catalog_replaced(self.chat.catalog.as_ref(), &catalog, &self.chat.models);
        } else {
            // Metadata replies can arrive after arrow-key input. Keep the highlighted identifier
            // across refresh/reordering instead of moving Enter's target back to the first row.
            self.chat.model_selection = self
                .chat
                .catalog
                .as_ref()
                .filter(|previous| previous.profile() == catalog.profile())
                .and_then(|previous| previous.models().get(self.chat.model_selection))
                .and_then(|selected| {
                    catalog.models().iter().position(|model| model.id() == selected.id())
                })
                .or_else(|| {
                    let current = self.chat.model_role.choice(&self.chat.models).id();
                    catalog.models().iter().position(|model| model.id() == current)
                })
                .unwrap_or(0);
        }
        self.chat.catalog = Some(catalog);
    }

    fn choose_model_with_effort(
        &mut self,
        id: &str,
        manual: bool,
        requested: Option<peritus_app_protocol::ProductModelEffort>,
    ) -> Vec<Effect> {
        if self.chat_submission_pending() {
            self.notice(
                NoticeLevel::Warning,
                "Wait for the pending conversation update before changing its model.",
            );
            return Vec::new();
        }
        if !manual
            && !self.chat.catalog.as_ref().is_some_and(|catalog| {
                let profile = self.chat_providers().map(|providers| match self.chat.model_role {
                    ModelRole::Writer => providers.writer(),
                    ModelRole::Reviewer => providers.reviewer(),
                    ModelRole::Fixer => providers.fixer(),
                });
                Some(catalog.profile()) == profile
                    && catalog.models().iter().any(|model| model.id() == id)
            })
        {
            self.notice(NoticeLevel::Warning, "Model is not in this provider's catalog. Open /model or explicitly use /model manual ID.");
            return Vec::new();
        }
        let choice = match ProductModelChoice::new(id.to_owned(), manual) {
            Ok(choice) => choice,
            Err(error) => {
                self.notice(NoticeLevel::Warning, error.to_string());
                return Vec::new();
            }
        };
        let effort =
            requested.unwrap_or_else(|| self.chat.model_role.choice(&self.chat.models).effort());
        self.save_model_choice(self.chat.model_role, choice.with_effort(effort))
    }

    fn save_model_choice(&mut self, role: ModelRole, choice: ProductModelChoice) -> Vec<Effect> {
        if self.chat_submission_pending() {
            self.notice(
                NoticeLevel::Warning,
                "Wait for the pending conversation update before changing model or effort.",
            );
            return Vec::new();
        }
        let description = format!(
            "{} · effort {} ({})",
            if choice.id().is_empty() { "configured model" } else { choice.id() },
            choice.effort().label(),
            if choice.manual() {
                "manual ID; availability unverified"
            } else {
                "model-specific effort support unverified"
            },
        );
        let writer_binding_changed =
            role == ModelRole::Writer && self.chat.models.writer() != &choice;
        let models = &self.chat.models;
        let models = match role {
            ModelRole::Writer => {
                ProductRoleModels::new(choice, models.reviewer().clone(), models.fixer().clone())
            }
            ModelRole::Reviewer => {
                ProductRoleModels::new(models.writer().clone(), choice, models.fixer().clone())
            }
            ModelRole::Fixer => {
                ProductRoleModels::new(models.writer().clone(), models.reviewer().clone(), choice)
            }
        };
        self.clear_chat_command();
        self.cancel_model_setup();
        if let Some(run_id) = self.chat.run_id {
            let effect = self.request(
                AppRequestPayload::UpdateModels(peritus_app_protocol::ProductModelUpdate::new(
                    run_id, models,
                )),
                PendingRequest::ModelUpdate { run_id },
            );
            if effect.is_some() {
                self.notice(
                    NoticeLevel::Info,
                    "Saving model and effort selection; waiting for daemon confirmation.",
                );
            } else {
                self.notice(
                    NoticeLevel::Error,
                    "Model/effort selection was not sent. Reconnect and select it again.",
                );
            }
            return effect.into_iter().collect();
        }
        self.chat.models = models;
        if writer_binding_changed {
            self.chat.workbench.files.discard_preview();
            self.chat.workbench.images.discard_preview();
        }
        self.notice(
            NoticeLevel::Info,
            format!("{} selection for new conversation: {description}", role.label()),
        );
        Vec::new()
    }
}
