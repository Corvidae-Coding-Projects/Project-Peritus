//! Host services borrowed by an independently compiled Rust plugin.
use super::{ProductRunInput, ProductRunnerError, error, runtime::Runtime};
use crate::developer_tools::{WorkspaceDeveloperTools, tool_definition};
use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::{CompletedToolCall, Message, ToolDefinition};
use peritus_plugin_sdk::ModuleAgentRequest;
use serde_json::{Value, json};

pub(super) struct NativeTools<'a> {
    runtime: &'a Runtime,
    input: &'a ProductRunInput,
    request: &'a ModuleAgentRequest,
    revision: u64,
    workspace: WorkspaceDeveloperTools,
    completed: bool,
}
impl<'a> NativeTools<'a> {
    pub(super) fn new(
        runtime: &'a Runtime,
        input: &'a ProductRunInput,
        request: &'a ModuleAgentRequest,
        revision: u64,
    ) -> Self {
        Self {
            runtime,
            input,
            request,
            revision,
            workspace: input.configure_tools(
                WorkspaceDeveloperTools::read_only(input.workspace_root.clone())
                    .with_task_contract(&request.prompt),
            ),
            completed: false,
        }
    }
    pub(super) fn definitions(
        &self,
        maximum: &[String],
    ) -> Result<Vec<ToolDefinition>, ProductRunnerError> {
        let catalog = crate::developer_tools::read_only_definitions()?;
        let mut definitions = vec![];
        let mut names = std::collections::BTreeSet::new();
        for name in &self.request.tools {
            if !maximum.contains(name) || !names.insert(name.clone()) {
                return Err(error("plugin agent requested an undeclared or duplicate host tool"));
            }
            definitions.push(
                catalog
                    .iter()
                    .find(|d| d.name().as_str() == name)
                    .ok_or_else(|| {
                        error(format!("host service {name} is unavailable to this native module"))
                    })?
                    .clone(),
            );
        }
        for function in &self.request.functions {
            if catalog.iter().any(|d| d.name().as_str() == function.name)
                || !names.insert(function.name.clone())
            {
                return Err(error(
                    "native plugin function cannot replace a host service or duplicate a tool name",
                ));
            }
            definitions.push(tool_definition(
                &function.name,
                &function.description,
                &function.schema.to_string(),
            )?);
        }
        Ok(definitions)
    }
}
impl DeveloperToolExecutor for NativeTools<'_> {
    fn yields_to_host(&self) -> bool {
        self.completed
    }
    fn system_guidance(&mut self) -> Option<String> {
        self.workspace.system_guidance()
    }
    fn observe_model_context(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.workspace.observe_model_context(messages)
    }
    fn take_progress_feedback(&mut self) -> Option<String> {
        self.workspace.take_progress_feedback()
    }
    fn effect(&self, call: &CompletedToolCall) -> DeveloperToolEffect {
        if self.request.functions.iter().any(|function| function.name == call.name().as_str()) {
            // Arbitrary native feature code may have effects; preserve pause-before-edit control.
            DeveloperToolEffect::MutationCapable
        } else {
            self.workspace.effect(call)
        }
    }
    fn observation_images(
        &mut self,
        call: &CompletedToolCall,
        observation: &DeveloperToolObservation,
    ) -> Result<Vec<peritus_model_protocol::MediaInput>, DeveloperLoopError> {
        self.workspace.observation_images(call, observation)
    }
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        self.execute_with_read(call, None)
    }
    fn reuse_workspace_read(
        &mut self,
        call: &CompletedToolCall,
        observation: &DeveloperToolObservation,
    ) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
        if call.name().as_str() != "workspace_read" {
            return Ok(None);
        }
        self.execute_with_read(call, Some(observation)).map(Some)
    }
}

impl NativeTools<'_> {
    fn execute_with_read(
        &mut self,
        call: &CompletedToolCall,
        reused: Option<&DeveloperToolObservation>,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        if self.input.provider_cancellation.is_cancelled()
            || self.input.cancelled.load(std::sync::atomic::Ordering::Acquire)
            || self.input.conversation.revision() != self.revision
        {
            return Err(DeveloperLoopError::Tool("native module tool cancelled".into()));
        }
        if !self
            .input
            .conversation
            .effective_permissions()
            .allows(crate::control::PermissionCapability::Read)
        {
            return crate::developer_tools::module_observation(
                &json!({"error":"current read permission is required"}),
                true,
            );
        }
        let selection = peritus_modules::Registry::for_user()
            .and_then(|r| r.load(&self.runtime.module_id, true));
        if !selection.is_ok_and(|d| d.digest == self.runtime.module_digest) {
            return crate::developer_tools::module_observation(
                &json!({"error":"native plugin is disabled, removed or changed; no host operation was executed"}),
                true,
            );
        }
        let name = call.name().as_str();
        if self.request.functions.iter().any(|f| f.name == name) {
            let result=self.runtime.call_sync(self.input,"tool",json!({"key":self.request.key,"name":name,"arguments_json":std::str::from_utf8(call.arguments().canonical_bytes()).map_err(|e|DeveloperLoopError::Tool(e.to_string()))?}),self.revision).map_err(|e|DeveloperLoopError::Tool(e.to_string()))?;
            let is_error = result.get("error").is_some();
            // Only a successful plugin-defined operation may commit its own stage.
            // Host web results cannot request this handoff. The coordinator still
            // validates its durable output before advancing to another stage.
            self.completed = !is_error && result["request_complete"] == true;
            return crate::developer_tools::module_observation(&result, is_error);
        }
        if !self.request.tools.iter().any(|t| t == name) {
            return crate::developer_tools::module_observation(
                &json!({"error":"tool not exported or requested by this plugin agent"}),
                true,
            );
        }
        let cached = reused
            .map(|value| self.workspace.reuse_workspace_read(call, value))
            .transpose()?
            .flatten();
        let observation = match cached {
            Some(value) => value,
            None => self.workspace.execute(call)?,
        };
        let result: Value = serde_json::from_slice(observation.output.canonical_bytes())
            .map_err(|e| DeveloperLoopError::Tool(e.to_string()))?;
        let retained=self.runtime.call_sync(self.input,"observe",json!({"key":self.request.key,"name":name,"arguments_json":std::str::from_utf8(call.arguments().canonical_bytes()).map_err(|e|DeveloperLoopError::Tool(e.to_string()))?,"result_json":result.to_string(),"is_error":observation.is_error}),self.revision).map_err(|e|DeveloperLoopError::Tool(e.to_string()))?;
        if let Some(error) = retained.get("error") {
            return crate::developer_tools::module_observation(
                &json!({"error":format!("The host operation completed but the plugin did not retain its observation: {error}. Retry the retained source read; do not claim this evidence was saved."),"source":result["source"]}),
                true,
            );
        }
        Ok(observation)
    }
}
