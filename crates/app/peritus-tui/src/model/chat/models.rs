//! Exact provider-discovered selection; manual identifiers are always explicitly marked.

use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest};
use crossterm::event::{KeyCode, KeyEvent};
use peritus_app_protocol::{
    AppRequestPayload, ProductModelCatalog, ProductModelChoice, ProductModelQuery,
    ProductRoleModels,
};

#[derive(Clone, Copy, Debug)]
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
    const fn next(self) -> Self {
        match self {
            Self::Writer => Self::Reviewer,
            Self::Reviewer => Self::Fixer,
            Self::Fixer => Self::Writer,
        }
    }
}

impl AppModel {
    pub(super) fn model_command(&mut self, arguments: &str) -> Vec<Effect> {
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
            Some("manual") => {
                if let Some(id) = words.next()
                    && words.next().is_none()
                {
                    return self.choose_model(id, true);
                }
                self.notice(
                    NoticeLevel::Warning,
                    "Usage: /model [writer|reviewer|fixer] manual MODEL_ID",
                );
                Vec::new()
            }
            None | Some("refresh") if words.next().is_none() => {
                self.clear_chat_command();
                self.chat.model_picker = true;
                self.query_chat_models(command == Some("refresh"))
            }
            Some(id) if words.next().is_none() => self.choose_model(id, false),
            _ => {
                self.notice(
                    NoticeLevel::Warning,
                    "Usage: /model [writer|reviewer|fixer] [refresh|MODEL_ID|manual MODEL_ID]",
                );
                Vec::new()
            }
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
        if self.chat.catalog.as_ref().is_some_and(|catalog| catalog.profile() != profile) {
            self.chat.catalog = None;
        }
        self.request(
            AppRequestPayload::QueryModels(ProductModelQuery::new(profile, refresh)),
            PendingRequest::ModelQuery,
        )
        .into_iter()
        .collect()
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
            .unwrap_or(0);
        self.chat.catalog = Some(catalog);
    }

    pub(super) fn model_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => self.chat.model_picker = false,
            KeyCode::Tab => {
                self.chat.model_role = self.chat.model_role.next();
                return self.query_chat_models(false);
            }
            KeyCode::Char('r') => return self.query_chat_models(true),
            KeyCode::Up => self.chat.model_selection = self.chat.model_selection.saturating_sub(1),
            KeyCode::Home => self.chat.model_selection = 0,
            KeyCode::End => {
                self.chat.model_selection = self
                    .chat
                    .catalog
                    .as_ref()
                    .map_or(0, |catalog| catalog.models().len().saturating_sub(1));
            }
            KeyCode::Down => {
                self.chat.model_selection = (self.chat.model_selection + 1).min(
                    self.chat
                        .catalog
                        .as_ref()
                        .map_or(0, |catalog| catalog.models().len().saturating_sub(1)),
                );
            }
            KeyCode::Enter => {
                if let Some(id) = self
                    .chat
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.models().get(self.chat.model_selection))
                    .map(|model| model.id().to_owned())
                {
                    return self.choose_model(&id, false);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn choose_model(&mut self, id: &str, manual: bool) -> Vec<Effect> {
        if self.chat_work_active() {
            self.notice(NoticeLevel::Warning, "Stop active work before changing its model.");
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
        let models = &self.chat.models;
        self.chat.models = match self.chat.model_role {
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
        self.chat.model_picker = false;
        self.notice(
            NoticeLevel::Info,
            format!(
                "{} model: {id} ({})",
                self.chat.model_role.label(),
                if manual {
                    "explicit manual ID; availability unverified"
                } else {
                    "provider-advertised; capabilities not probed"
                }
            ),
        );
        Vec::new()
    }
}
