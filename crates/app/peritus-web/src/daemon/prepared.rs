//! Exact prepared message bodies, external selections, and original native ownership.

use super::{
    NativeOwner, Result, Value, bytes, hex, interaction_mode, json, model_values, problem, role_models,
};
use peritus_app_protocol::{
    ConversationId, ConversationTitle, ProductInteractionMode, ProductProviderSelection,
    ProductRoleModels, WorkbenchQuery,
};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedChat {
    pub(super) query: WorkbenchQuery,
    pub(super) run: RunId,
    pub(super) title: ConversationTitle,
    pub(super) providers: ProductProviderSelection,
    pub(super) mode: ProductInteractionMode,
    pub(super) models: ProductRoleModels,
    pub(super) text: String,
    pub(super) attachments: Vec<crate::files::attachments::Attachment>,
    pub(super) owner: Option<NativeOwner>,
}


impl PreparedChat {
    pub(super) fn retained(&self) -> Result<Value> {
        Ok(json!({
            "version":3,
            "conversation":hex(self.query.conversation().as_bytes()),
            "workspace":hex(self.query.workspace().as_bytes()),
            "run":hex(self.run.as_bytes()),
            "title":self.title.as_str(),
            "providers":{
                "writer":hex(self.providers.writer().as_bytes()),
                "reviewer":hex(self.providers.reviewer().as_bytes()),
                "fixer":hex(self.providers.fixer().as_bytes())
            },
            "mode":format!("{:?}",self.mode).to_lowercase(),
            "models":model_values(&self.models),
            "text":self.text,
            "attachments":self.attachments,
            "owner":self.owner()?.retained()?
        }))
    }
    pub(super) fn from_retained(value: &Value) -> Result<Self> {
        match value["version"].as_u64() {
            Some(1) => Self::from_legacy_retained(value),
            Some(2) => Self::from_owner_retained(value),
            Some(3) => Self::from_owner_retained(value),
            _ => Err(problem("Unsupported prepared message context")),
        }
    }
    fn from_owner_retained(value: &Value) -> Result<Self> {
        Self::decode_retained(value, Some(NativeOwner::decode(&value["owner"])?))
    }
    fn from_legacy_retained(value: &Value) -> Result<Self> {
        Self::decode_retained(value, None)
    }
    fn decode_retained(value: &Value, owner: Option<NativeOwner>) -> Result<Self> {
        let identity = |name: &str| -> Result<[u8; 16]> {
            bytes(
                value[name]
                    .as_str()
                    .ok_or_else(|| problem("Incomplete prepared message context"))?,
            )
        };
        let provider = |role: &str| -> Result<ProviderProfileId> {
            ProviderProfileId::new(bytes(
                value["providers"][role]
                    .as_str()
                    .ok_or_else(|| problem("Incomplete prepared provider context"))?,
            )?)
            .map_err(|error| problem(format!("{error:?}")))
        };
        let prepared = Self {
            query: WorkbenchQuery::new(
                ConversationId::new(identity("conversation")?)
                    .map_err(|error| problem(format!("{error:?}")))?,
                WorkspaceId::new(identity("workspace")?)
                    .map_err(|error| problem(format!("{error:?}")))?,
            ),
            run: RunId::new(identity("run")?).map_err(|error| problem(format!("{error:?}")))?,
            title: crate::sessions::title(
                value["title"]
                    .as_str()
                    .ok_or_else(|| problem("Incomplete prepared message title"))?,
            )?,
            providers: ProductProviderSelection::new(
                provider("writer")?,
                provider("reviewer")?,
                provider("fixer")?,
            ),
            mode: interaction_mode(
                value["mode"]
                    .as_str()
                    .ok_or_else(|| problem("Incomplete prepared message mode"))?,
            )?,
            models: role_models(&value["models"])?,
            text: value["text"]
                .as_str()
                .ok_or_else(|| problem("Incomplete prepared message text"))?
                .to_owned(),
            attachments: if value["version"] == 3 {
                serde_json::from_value(value["attachments"].clone()).map_err(problem)?
            } else { Vec::new() },
            owner,
        };
        if prepared.owner.as_ref().is_some_and(|owner| {
            owner.workspace() != hex(prepared.query.workspace().as_bytes())
        }) {
            return Err(problem(
                "Prepared message workspace does not match its retained native owner",
            ));
        }
        Ok(prepared)
    }
    pub(super) fn owner(&self) -> Result<&NativeOwner> {
        self.owner.as_ref().ok_or_else(|| {
            crate::error::uncertain(
                "The legacy prepared message has no native owner binding. Its retained evidence remains inspectable, but it cannot be retried or transmitted.",
            )
        })
    }
}
