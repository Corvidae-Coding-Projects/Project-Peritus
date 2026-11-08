//! One owner for destination checks shared by guided and direct model selection.
use super::{AppModel, ModelRole};
use peritus_app_protocol::{ProductProviderSelection, WorkbenchQuery};
use peritus_types::{ProviderProfileId, RunId};

#[derive(Debug)]
pub(super) struct ModelDestination {
    run: Option<RunId>,
    conversation: Option<WorkbenchQuery>,
    connection: u64,
    providers: ProductProviderSelection,
}
impl ModelDestination {
    pub(super) fn capture(model: &AppModel) -> Option<Self> {
        Some(Self {
            run: model.chat.run_id,
            conversation: model.chat.workbench.selected,
            connection: model.connection_generation,
            providers: model.chat_providers()?,
        })
    }
    pub(super) const fn provider(&self, role: ModelRole) -> ProviderProfileId {
        match role {
            ModelRole::Writer => self.providers.writer(),
            ModelRole::Reviewer => self.providers.reviewer(),
            ModelRole::Fixer => self.providers.fixer(),
        }
    }
    fn same_conversation(&self, model: &AppModel, role: ModelRole) -> bool {
        self.run == model.chat.run_id
            && self.conversation == model.chat.workbench.selected
            && self.connection == model.connection_generation
            && role == model.chat.model_role
            && model.chat.buffer.is_empty()
    }
    pub(super) fn matches_menu(&self, model: &AppModel, role: ModelRole) -> bool {
        self.same_conversation(model, role) && Some(self.providers) == model.chat_providers()
    }
    pub(super) fn matches_role(&self, model: &AppModel, role: ModelRole) -> bool {
        self.same_conversation(model, role) && Some(self.provider(role)) == model.role_profile()
    }
}
