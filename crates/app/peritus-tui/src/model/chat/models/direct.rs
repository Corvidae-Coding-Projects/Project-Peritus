//! Direct role commands load their catalog and save model plus effort in one transaction.
use super::{AppModel, Effect, ModelRole, NoticeLevel};
use peritus_app_protocol::{ProductModelEffort, WorkbenchQuery};
use peritus_types::{ProviderProfileId, RunId};

#[derive(Debug)]
pub(in crate::model::chat) struct PendingModelSelection {
    role: ModelRole,
    profile: ProviderProfileId,
    run: Option<RunId>,
    conversation: Option<WorkbenchQuery>,
    connection: u64,
    id: String,
    effort: Option<ProductModelEffort>,
    command: String,
}

impl AppModel {
    pub(super) fn direct_model_words<'a>(
        &mut self,
        mut words: impl Iterator<Item = &'a str>,
        manual: bool,
    ) -> Vec<Effect> {
        let Some(id) = words.next() else { return self.model_usage() };
        let effort = match words.next() {
            None => None,
            Some(value) => match ProductModelEffort::parse(value) {
                Some(effort) => Some(effort),
                None => return self.model_usage(),
            },
        };
        if words.next().is_some() {
            return self.model_usage();
        }
        if self.chat_submission_pending() {
            self.notice(
                NoticeLevel::Info,
                "Waiting for the previous selection receipt; command retained.",
            );
            return Vec::new();
        }
        let Some(profile) = self.role_profile() else {
            self.notice(
                NoticeLevel::Warning,
                "Open a configured project before selecting role models.",
            );
            return Vec::new();
        };
        if manual
            || self.chat.catalog.as_ref().is_some_and(|catalog| {
                catalog.profile() == profile && catalog.models().iter().any(|m| m.id() == id)
            })
        {
            return self.choose_model_with_effort(id, manual, effort);
        }
        if self.context.is_none() {
            self.notice(NoticeLevel::Warning, "Disconnected; model command retained.");
            return Vec::new();
        }
        let command = self.chat.buffer.clone();
        let refresh =
            self.chat.catalog.as_ref().is_some_and(|catalog| catalog.profile() == profile);
        self.clear_chat_command();
        self.chat.show_model_picker();
        let effects = self.query_chat_models(refresh);
        self.chat.pending_model = Some(PendingModelSelection {
            role: self.chat.model_role,
            profile,
            run: self.chat.run_id,
            conversation: self.chat.workbench.selected,
            connection: self.connection_generation,
            id: id.to_owned(),
            effort,
            command,
        });
        self.notice(NoticeLevel::Info,"Checking this role's model catalog; the explicit model and effort will be saved together.");
        effects
    }

    pub(super) fn model_usage(&mut self) -> Vec<Effect> {
        self.notice(NoticeLevel::Warning,"Enter /model to choose the agent, model, and effort from menus. Optional shortcut: /model ROLE [manual] MODEL_ID EFFORT.");
        Vec::new()
    }

    pub(super) fn role_profile(&self) -> Option<ProviderProfileId> {
        let providers = self.chat_providers()?;
        Some(match self.chat.model_role {
            ModelRole::Writer => providers.writer(),
            ModelRole::Reviewer => providers.reviewer(),
            ModelRole::Fixer => providers.fixer(),
        })
    }

    pub(in crate::model) fn model_lookup_failed(&mut self) {
        if let Some(selection) = self.chat.pending_model.take()
            && self.model_destination_matches(&selection)
        {
            self.chat.buffer = selection.command;
            self.chat.cursor = self.chat.buffer.len();
        }
    }

    fn model_destination_matches(&self, selection: &PendingModelSelection) -> bool {
        selection.role == self.chat.model_role
            && Some(selection.profile) == self.role_profile()
            && selection.run == self.chat.run_id
            && selection.conversation == self.chat.workbench.selected
            && selection.connection == self.connection_generation
            && self.chat.buffer.is_empty()
    }

    pub(super) fn finish_direct_model_selection(&mut self) -> Vec<Effect> {
        let Some(selection) = self.chat.pending_model.take() else { return Vec::new() };
        if !self.model_destination_matches(&selection) {
            self.notice(NoticeLevel::Info,"Model lookup finished after the destination or draft changed; no selection was applied.");
            return Vec::new();
        }
        if !self.chat.catalog.as_ref().is_some_and(|catalog| {
            catalog.profile() == selection.profile
                && catalog.models().iter().any(|m| m.id() == selection.id)
        }) {
            self.chat.buffer = selection.command;
            self.chat.cursor = self.chat.buffer.len();
            self.notice(NoticeLevel::Warning,"That model was not advertised by this role's provider. Refresh or explicitly use /model ROLE manual ID EFFORT; command retained.");
            return Vec::new();
        }
        if self.chat_submission_pending() {
            self.chat.buffer = selection.command;
            self.chat.cursor = self.chat.buffer.len();
            self.notice(
                NoticeLevel::Info,
                "Waiting for a pending receipt; model command retained for resubmission.",
            );
            return Vec::new();
        }
        self.choose_model_with_effort(&selection.id, false, selection.effort)
    }
}
