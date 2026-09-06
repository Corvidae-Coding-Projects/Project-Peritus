//! Shared context port; locks do not cross task-provider requests or base-tool execution.

use super::{
    assembly::{MEMORY_POLICY, text_message},
    error,
    memory::LocalMemory,
};
use crate::{ConversationView, ProductRunInput, ProductRunnerError};
use peritus_agent::{
    DeveloperContextAssembly, DeveloperContextEvent, DeveloperContextPort, DeveloperLoopError,
    DeveloperLoopRequest,
};
use peritus_context::{ContextNodeId, working::WorkingBinding};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, Message, ProtocolLimits, Role,
};
use peritus_role::HarnessRole;
use peritus_types::Sha256Digest;
use serde_json::Value;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct LocalContextHandle {
    inner: Arc<Mutex<LocalMemory>>,
    conversation: Arc<dyn ConversationView>,
}

impl LocalContextHandle {
    pub(crate) fn open(
        input: &ProductRunInput,
        role: &str,
    ) -> Result<Option<Self>, ProductRunnerError> {
        let config = input.command_runtime.local_context_config().clone();
        if !config.enabled {
            return Ok(None);
        }
        let harness_role = match role {
            "writer" => HarnessRole::Writer,
            "fixer" => HarnessRole::Fixer,
            "reviewer" => HarnessRole::Reviewer,
            _ => return Err(crate::turn::developer_error(&error("unsupported local memory role"))),
        };
        let task = ContextNodeId::new(input.run_id.into_bytes())
            .map_err(|_| crate::turn::developer_error(&error("invalid logical task identity")))?;
        let binding = WorkingBinding::new(
            input.run_id,
            input.workspace_id,
            task,
            harness_role,
            input.conversation.revision(),
        );
        let root = input.trace_path.with_extension("context").join(role);
        let mut memory =
            LocalMemory::load(&root, &input.workspace_root, &input.trace_path, binding, config)
                .map_err(|error| crate::turn::developer_error(&error))?;
        memory.compactor_runtime = Some(input.command_runtime.clone());
        Ok(Some(Self {
            inner: Arc::new(Mutex::new(memory)),
            conversation: Arc::clone(&input.conversation),
        }))
    }

    pub(in crate::local_context) fn lock(
        &self,
    ) -> Result<MutexGuard<'_, LocalMemory>, DeveloperLoopError> {
        self.inner.lock().map_err(|_| error("local memory owner is poisoned"))
    }
    pub(crate) fn scope_digest(&self) -> Result<Sha256Digest, DeveloperLoopError> {
        Ok(self.lock()?.store.scope_digest())
    }
    pub(crate) fn next_invocation(&self) -> Result<u64, DeveloperLoopError> {
        self.lock()?
            .transcript
            .invocation
            .checked_add(1)
            .ok_or_else(|| error("invocation sequence overflow"))
    }
}

impl DeveloperContextPort for LocalContextHandle {
    fn source_reference(
        &self,
        call: &CompletedToolCall,
    ) -> Result<Option<CanonicalJson>, DeveloperLoopError> {
        let memory = self.lock()?;
        let source = memory
            .sources
            .iter()
            .rev()
            .find(|source| {
                source.invocation == memory.transcript.invocation
                    && source
                        .call
                        .as_ref()
                        .is_some_and(|identity| identity.id == call.id().expose_for_wire())
            })
            .ok_or_else(|| error("original tool source unavailable"))?;
        let value = Value::from_iter([
            ("handle", Value::from(super::tools::source_handle(&memory, source.sequence))),
            ("base_revision", Value::from(memory.model_revision)),
            ("sha256", Value::from(super::tools::hex(source.artifact.digest.as_bytes()))),
            ("bytes", Value::from(source.artifact.bytes)),
            ("authority", Value::from("none")),
        ]);
        drop(memory);
        Ok(Some(CanonicalJson::parse(
            &value.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?))
    }

    fn open(
        &mut self,
        request: &DeveloperLoopRequest,
        initial_messages: &[Message],
    ) -> Result<(), DeveloperLoopError> {
        let mut memory = self.lock()?;
        let binding = memory.binding;
        memory.binding = WorkingBinding::new(
            binding.run(),
            binding.workspace(),
            binding.task(),
            binding.role(),
            self.conversation.revision(),
        );
        memory.begin(request, initial_messages)?;
        memory.observe_message(&text_message(Role::Developer, MEMORY_POLICY.to_owned())?)?;
        drop(memory);
        Ok(())
    }
    fn observe(&mut self, event: DeveloperContextEvent<'_>) -> Result<(), DeveloperLoopError> {
        let mut memory = self.lock()?;
        match event {
            DeveloperContextEvent::Message(message) => {
                memory.observe_message(message)?;
            }
            DeveloperContextEvent::ToolObservation { call, observation } => {
                memory.observe_tool(call, observation)?;
            }
            DeveloperContextEvent::BatchCompleted => {
                memory.compact_locally()?;
                if memory.config.checkpoint_every_completed_batch
                    && let Some(profile) = memory.profile.clone()
                {
                    let tools = memory.tools.clone();
                    let messages = memory.prepare_view(&profile, &tools)?;
                    memory.publish(&messages)?;
                }
            }
        }
        drop(memory);
        Ok(())
    }
    fn assemble(
        &mut self,
        request: DeveloperContextAssembly<'_>,
    ) -> Result<Vec<Message>, DeveloperLoopError> {
        self.lock()?.prepare_view(request.profile, request.tools)
    }
    fn checkpoint(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.lock()?.publish(messages)
    }
}
