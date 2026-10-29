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

/// Exclusive durable local-context owner shared through the production context port.
/// Clones share the owner; dropping the final handle releases its storage lock.
#[derive(Clone)]
pub struct LocalContextHandle {
    inner: Arc<Mutex<LocalMemory>>,
    conversation: Arc<dyn ConversationView>,
}

impl LocalContextHandle {
    /// Recovers the same lineage selected by the production run's delivery adapter.
    ///
    /// In-place writer and fixer invocations share the writer lineage. This may append
    /// recovered host projections, but never dispatches effects or invokes a provider.
    ///
    /// # Errors
    /// Rejects invalid storage, unavailable ownership, or inconsistent durable recovery.
    pub fn for_run(
        input: &ProductRunInput,
        role: &str,
    ) -> Result<Option<Self>, ProductRunnerError> {
        input.working_memory(role)
    }

    /// Estimates existing literal pins and complete required working-state framing.
    ///
    /// # Errors
    /// Rejects invalid scope, unavailable artifacts, or a structurally invalid projection.
    pub fn additional_request_framing_tokens(&self) -> Result<u64, DeveloperLoopError> {
        self.lock()?.additional_request_framing_tokens()
    }

    /// Returns exact memory tool definitions for this owner's configured policy.
    ///
    /// # Errors
    /// Rejects poisoned ownership or invalid bounded schemas.
    pub fn tool_definitions(
        &self,
    ) -> Result<Vec<peritus_model_protocol::ToolDefinition>, DeveloperLoopError> {
        let memory = self.lock()?;
        let mut definitions = super::tools::definitions_with_config(&memory.config)?;
        if !memory.config.read_only_plans.is_enabled()
            && memory.checkpoint_schema >= super::record::PLAN_CHECKPOINT_SCHEMA_VERSION
        {
            definitions.push(super::operation_plan::read_definition()?);
        }
        drop(memory);
        Ok(definitions)
    }

    pub(in crate::local_context) fn effective_permissions(
        &self,
    ) -> crate::control::HostPermissions {
        self.conversation.effective_permissions()
    }

    pub(crate) fn open(
        input: &ProductRunInput,
        role: &str,
    ) -> Result<Option<Self>, ProductRunnerError> {
        Self::open_scoped(input, role, None)
    }

    pub(crate) fn open_folder(
        input: &ProductRunInput,
        protected: &[std::path::PathBuf],
    ) -> Result<Option<Self>, ProductRunnerError> {
        Self::open_scoped(input, "writer", Some(protected))
    }

    pub(crate) fn open_folder_for_role(
        input: &ProductRunInput,
        role: &str,
        protected: &[std::path::PathBuf],
    ) -> Result<Option<Self>, ProductRunnerError> {
        Self::open_scoped(input, role, Some(protected))
    }

    /// Module stages have their own task identity even in an ordinary in-place folder.
    /// Never route a dynamic stage label through the coding writer's default lineage.
    pub(crate) fn open_module(
        input: &ProductRunInput,
        job_key: &str,
        stage: &str,
        role: peritus_agent::DeveloperModelRole,
    ) -> Result<Option<Self>, ProductRunnerError> {
        let config = input.command_runtime.local_context_config().clone();
        if !config.enabled {
            return Ok(None);
        }
        let mut identity = b"peritus-module-stage-context-v2\0".to_vec();
        identity.extend_from_slice(input.run_id.as_bytes());
        identity.extend_from_slice(job_key.as_bytes());
        identity.push(0);
        identity.extend_from_slice(stage.as_bytes());
        let hash = peritus_codec::sha256(&identity);
        let mut task = [0; 16];
        task.copy_from_slice(&hash.as_bytes()[..16]);
        task[0] |= 1;
        let task = ContextNodeId::new(task)
            .map_err(|_| crate::turn::developer_error(&error("invalid module task identity")))?;
        let role = match role {
            peritus_agent::DeveloperModelRole::Reviewer => HarnessRole::Reviewer,
            peritus_agent::DeveloperModelRole::Writer => HarnessRole::Writer,
            peritus_agent::DeveloperModelRole::Fixer => HarnessRole::Fixer,
        };
        let revision = match input.conversation.interaction() {
            Some(port) => {
                port.input().map_err(|error| crate::turn::developer_error(&error))?.revision
            }
            None => input.conversation.revision(),
        };
        let binding = WorkingBinding::new(input.run_id, input.workspace_id, task, role, revision);
        let namespace = crate::encode_hex(hash.as_bytes());
        let root = input.trace_path.with_extension("context").join("modules").join(namespace);
        // A native module role receives only the module's prompt and declared tools. Its durable
        // coordinator owns source observations, so enrolling or fingerprinting the host workspace
        // here both duplicates that evidence and accidentally exposes ambient coding context.
        // Keep the context lineage and transcript, but bind it to an empty explicit file scope.
        Self::open_bound(input, &root, binding, config, Some(&[]))
    }

    fn open_scoped(
        input: &ProductRunInput,
        role: &str,
        protected: Option<&[std::path::PathBuf]>,
    ) -> Result<Option<Self>, ProductRunnerError> {
        let config = input.command_runtime.local_context_config().clone();
        if !config.enabled {
            return Ok(None);
        }
        let harness_role = match role {
            "writer" => HarnessRole::Writer,
            "fixer" => HarnessRole::Fixer,
            "reviewer" | "designer" | "design-interview" => HarnessRole::Reviewer,
            _ => return Err(crate::turn::developer_error(&error("unsupported local memory role"))),
        };
        // Design is a separate read-only subtask. Its handles must never resolve in the
        // independent review archive, even though both exclude writer-derived memory.
        let task_bytes = if matches!(role, "designer" | "design-interview") {
            let mut key = if role == "designer" {
                b"peritus-read-only-design-context-v1".to_vec()
            } else {
                b"peritus-guided-design-context-v1".to_vec()
            };
            key.extend_from_slice(input.run_id.as_bytes());
            let digest = peritus_codec::sha256(&key);
            let mut task = [0; 16];
            task.copy_from_slice(&digest.as_bytes()[..16]);
            task[0] |= 1;
            task
        } else {
            input.run_id.into_bytes()
        };
        let task = ContextNodeId::new(task_bytes)
            .map_err(|_| crate::turn::developer_error(&error("invalid logical task identity")))?;
        let binding = WorkingBinding::new(
            input.run_id,
            input.workspace_id,
            task,
            harness_role,
            input.conversation.revision(),
        );
        let root = input.trace_path.with_extension("context").join(role);
        Self::open_bound(input, &root, binding, config, protected)
    }

    fn open_bound(
        input: &ProductRunInput,
        root: &std::path::Path,
        binding: WorkingBinding,
        config: crate::LocalContextConfig,
        protected: Option<&[std::path::PathBuf]>,
    ) -> Result<Option<Self>, ProductRunnerError> {
        let mut memory = if let Some(protected) = protected {
            LocalMemory::load_scoped(
                root,
                &input.workspace_root,
                &input.trace_path,
                binding,
                config,
                super::memory::environment::WorkspaceScope {
                    direct: true,
                    protected: protected.to_vec(),
                },
            )
            .map_err(|error| crate::turn::developer_error(&error))?
        } else {
            LocalMemory::load(root, &input.workspace_root, &input.trace_path, binding, config)
                .map_err(|error| crate::turn::developer_error(&error))?
        };
        memory.compactor_runtime = Some(input.command_runtime.clone());
        memory.conversation = Some(Arc::clone(&input.conversation));
        memory.cancellation =
            Some((Arc::clone(&input.cancelled), input.provider_cancellation.clone()));
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
        // A retrieval result already names the original source fragments and their offsets.
        // Advertising the receipt itself as another task source creates an unbounded chain
        // of reads of JSON wrappers. Preserve it in the audit archive without that invitation.
        if source.is_memory_receipt() {
            return Ok(None);
        }
        if source.call.as_ref().is_some_and(|call| call.name == "workspace_read")
            && serde_json::from_slice::<Value>(&memory.artifact(source.sequence)?)
                .ok()
                .is_some_and(|value| value["read_reuse"]["state"] == "already_visible")
        {
            // A reuse receipt points to original pages; never invite a chain of receipt reads.
            return Ok(None);
        }
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
            DeveloperContextEvent::HostObservation(message) => {
                memory.observe_host_message(message)?;
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
        let mut memory = self.lock()?;
        // A new provider view starts an unbound request epoch. Internal batch/reader-guard
        // checkpoints bypass this port and must preserve the active response's origin.
        memory.decision_dependencies = None;
        memory.published_dependencies = None;
        memory.prepare_view_with_governing(
            request.profile,
            request.tools,
            Some(request.invocation_policy),
            request.invocation_state,
            request.governing_input,
            Some(request.provider),
        )
    }
    fn checkpoint(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.lock()?.publish(messages)
    }
    fn bind_request(
        &mut self,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<(), DeveloperLoopError> {
        let mut memory = self.lock()?;
        if memory.host_dependencies_enabled() {
            memory.bind_host_dependencies(request)?;
        }
        if memory.config.decision_view == crate::DecisionViewPolicy::Conservative {
            super::assembly::decision::record_request(&memory, request)?;
        }
        drop(memory);
        Ok(())
    }
}
