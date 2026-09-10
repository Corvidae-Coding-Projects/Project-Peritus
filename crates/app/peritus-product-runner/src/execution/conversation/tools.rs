//! A read-only conversation can hand authorized implementation to the production pipeline.

use crate::{
    ConversationView, control::PermissionCapability, developer_tools::WorkspaceDeveloperTools,
};
use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::{
    BoundedText, CanonicalJson, CompletedToolCall, JsonBounds, JsonSchema, ProtocolLimits,
    SchemaDialect, ToolDefinition, ToolName,
};
use std::sync::Arc;

pub(super) struct ConversationTools {
    pub(super) workspace: WorkspaceDeveloperTools,
    pub(super) requested_revision: Option<u64>,
    pub(super) allow_pipeline: bool,
    pub(super) conversation: Arc<dyn ConversationView>,
}

impl ConversationTools {
    pub(super) fn new(input: &crate::ProductRunInput, allow_pipeline: bool) -> Self {
        Self {
            workspace: input.configure_tools(
                WorkspaceDeveloperTools::read_only(input.workspace_root.clone())
                    .with_task_contract(&input.conversation.render()),
            ),
            requested_revision: None,
            allow_pipeline,
            conversation: Arc::clone(&input.conversation),
        }
    }
}

impl DeveloperToolExecutor for ConversationTools {
    fn effect(&self, call: &CompletedToolCall) -> DeveloperToolEffect {
        if call.name().as_str() == "run_pipeline" {
            DeveloperToolEffect::MutationCapable
        } else {
            self.workspace.effect(call)
        }
    }

    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        if call.name().as_str() != "run_pipeline" {
            return self.workspace.execute(call);
        }
        let arguments: serde_json::Value =
            serde_json::from_slice(call.arguments().canonical_bytes())
                .map_err(|error| DeveloperLoopError::Tool(error.to_string()))?;
        let allowed = self.allow_pipeline
            && arguments.as_object().is_some_and(serde_json::Map::is_empty)
            && pipeline_permissions_allow(self.conversation.as_ref());
        if allowed {
            self.requested_revision = Some(self.conversation.incorporated_revision());
        }
        let output = if allowed {
            r#"{"handoff":"The host will run the existing design, writer, verification, reviewer and fixer pipeline for the unchanged user conversation."}"#
        } else {
            r#"{"error":"Pipeline handoff is unavailable in read-only mode or received invalid arguments."}"#
        };
        Ok(DeveloperToolObservation {
            output: CanonicalJson::parse(output, JsonBounds::value(ProtocolLimits::PRODUCTION))?,
            is_error: !allowed,
        })
    }

    fn yields_to_host(&self) -> bool {
        self.requested_revision.is_some()
    }
    // Conversation reads have no delivery obligations. All effects live in the pipeline executor,
    // which retains its normal grounding, gate, review and progress checks.
}

pub(super) fn pipeline_permissions_allow(conversation: &dyn ConversationView) -> bool {
    let permissions = conversation.effective_permissions();
    [
        PermissionCapability::Read,
        PermissionCapability::Write,
        PermissionCapability::Process,
        PermissionCapability::Network,
    ]
    .into_iter()
    .all(|capability| permissions.allows(capability))
}

pub(super) fn definition() -> Result<ToolDefinition, DeveloperLoopError> {
    let limits = ProtocolLimits::PRODUCTION;
    Ok(ToolDefinition::new(
        ToolName::new("run_pipeline".to_owned())?,
        Some(BoundedText::new("Hand an explicitly authorized implementation, fix or effectful task to the existing production pipeline. It performs design, implementation, exact-target checks, independent review and fixes; progress stays in this chat. The host preserves the actual user conversation and selected models. Do not use for questions, explanations, diagnosis, planning or read-only review. This does not authorize work beyond the user's request.".to_owned(), limits)?),
        JsonSchema::parse(r#"{"type":"object","properties":{},"required":[],"additionalProperties":false}"#, SchemaDialect::Draft202012, JsonBounds::schema(limits))?, true,
    ))
}
