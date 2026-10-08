//! Shared context port; locks do not cross task-provider requests or base-tool execution.

use super::{
    assembly::{MEMORY_POLICY, text_message},
    error,
    memory::LocalMemory,
    record::{
        CallIdentity, PendingState, SEGMENT_CONTINUATION_SCHEMA_VERSION, SegmentContinuation,
        SegmentPendingBatch, SegmentProgress,
    },
};
use crate::{
    ConversationView, ProductRunInput, ProductRunnerError,
    developer_tools::GroundingEvidence,
};
use peritus_agent::{
    DeveloperCompactionOwner, DeveloperContextAssembly, DeveloperContextEvent,
    DeveloperContextPort, DeveloperContextResume, DeveloperLoopError, DeveloperLoopProgress,
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
    session_directory: std::path::PathBuf,
}

struct ContextConstruction {
    root: std::path::PathBuf,
    workspace: std::path::PathBuf,
    trace: std::path::PathBuf,
    binding: WorkingBinding,
    config: super::LocalContextConfig,
    workspace_scope: super::memory::environment::WorkspaceScope,
    runtime: crate::CommandRuntime,
    conversation: Arc<dyn ConversationView>,
    cancellation: peritus_provider_core::CancellationToken,
}

struct OwnershipWaitGuard(Option<peritus_provider_core::CancellationToken>);

impl Drop for OwnershipWaitGuard {
    fn drop(&mut self) {
        if let Some(cancellation) = self.0.take() {
            let _ = cancellation.cancel();
        }
    }
}

impl LocalContextHandle {
    pub(crate) fn tool_definitions(
        &self,
    ) -> Result<Vec<peritus_model_protocol::ToolDefinition>, DeveloperLoopError> {
        super::tools::definitions_with_config(&self.lock()?.config)
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
        Self::open_scoped(
            input,
            role,
            input.workspace_kind.is_in_place().then(|| input.workspace_kind.protected_paths()),
        )
    }

    pub(crate) async fn open_async(
        input: &ProductRunInput,
        role: &str,
    ) -> Result<Option<Self>, ProductRunnerError> {
        crate::execution::check_cancelled(input)?;
        let protected = input.workspace_kind.is_in_place()
            .then(|| input.workspace_kind.protected_paths());
        let Some(construction) = ContextConstruction::capture(input, role, protected)? else {
            return Ok(None);
        };
        // Construction may wait indefinitely for the same lineage owner. Keep that wait and
        // SQLite/file I/O off the execution worker, while retaining the caller's cancellation.
        let mut waiting = OwnershipWaitGuard(Some(construction.cancellation.clone()));
        let mut constructing = tokio::task::spawn_blocking(move || construction.open());
        let result = loop {
            tokio::select! {
                result = &mut constructing => break result,
                () = tokio::time::sleep(std::time::Duration::from_millis(5)) => {
                    if input.cancelled.load(std::sync::atomic::Ordering::Acquire) {
                        // Legacy callers may set only the run flag. Forward it to the same
                        // provider token observed by the native owner and SQLite wait loops.
                        let _ = input.provider_cancellation.cancel();
                    }
                }
            }
        };
        waiting.0 = None;
        let context = result.map_err(|failure| {
            ProductRunnerError::new(
                crate::ProductRunnerErrorKind::InternalInvariant,
                "construct local context",
                format!("local context construction worker failed: {failure}"),
            )
        })??;
        crate::execution::check_cancelled(input)?;
        Ok(Some(context))
    }

    fn open_scoped(
        input: &ProductRunInput,
        role: &str,
        protected: Option<&[std::path::PathBuf]>,
    ) -> Result<Option<Self>, ProductRunnerError> {
        ContextConstruction::capture(input, role, protected)?
            .map(ContextConstruction::open).transpose()
    }

    pub(in crate::local_context) fn lock(
        &self,
    ) -> Result<MutexGuard<'_, LocalMemory>, DeveloperLoopError> {
        self.inner.lock().map_err(|_| error("local memory owner is poisoned"))
    }
    pub(crate) fn scope_digest(&self) -> Result<Sha256Digest, DeveloperLoopError> {
        Ok(self.lock()?.store.scope_digest())
    }
    pub(crate) fn invocation_scope(
        &self,
        request_prefix: &str,
    ) -> Result<u64, DeveloperLoopError> {
        self.lock()?.invocation_scope(request_prefix)
    }

    pub(crate) fn pending_reentry_prefix(
        &self,
        expected_prefix: &str,
    ) -> Result<Option<String>, DeveloperLoopError> {
        self.lock()?.pending_reentry_prefix(expected_prefix)
    }

    pub(crate) fn pending_developer_reentry(
        &self,
        expected_prefix: &str,
    ) -> Result<Option<(String, u64)>, DeveloperLoopError> {
        self.lock()?.pending_developer_reentry(expected_prefix)
    }

    pub(crate) fn recover_grounding(
        &self,
        expected_prefix: &str,
    ) -> Result<GroundingEvidence, DeveloperLoopError> {
        self.lock()?.recover_grounding(expected_prefix)
    }

    pub(crate) fn recover_grounding_scope(
        &self,
        expected_scope: &str,
        expected_revision: u64,
    ) -> Result<GroundingEvidence, DeveloperLoopError> {
        self.lock()?.recover_grounding_scope(expected_scope, expected_revision)
    }

    pub(crate) fn replay_tool_observations(
        &self,
        expected_prefix: &str,
        tool_name: &str,
        observe: &mut dyn FnMut(&CompletedToolCall, &CanonicalJson),
    ) -> Result<(), DeveloperLoopError> {
        self.lock()?.replay_tool_observations(expected_prefix, Some(tool_name), observe)
    }
}

impl ContextConstruction {
    fn capture(
        input: &ProductRunInput,
        role: &str,
        protected: Option<&[std::path::PathBuf]>,
    ) -> Result<Option<Self>, ProductRunnerError> {
        let config = input.command_runtime.local_context_config().clone();
        if !config.enabled {
            return Ok(None);
        }
        let harness_role = match role {
            "writer" | "designer" => HarnessRole::Writer,
            "fixer" => HarnessRole::Fixer,
            "reviewer" => HarnessRole::Reviewer,
            _ => return Err(crate::turn::developer_error(&error("unsupported local memory role"))),
        };
        let task_identity = if role == "designer" {
            let mut label = b"peritus/design-task/v1\0".to_vec();
            label.extend_from_slice(input.run_id.as_bytes());
            let digest = peritus_codec::sha256(&label);
            let mut identity = [0; 16];
            identity.copy_from_slice(&digest.as_bytes()[..16]);
            identity[0] |= 1;
            identity
        } else {
            input.run_id.into_bytes()
        };
        let task = ContextNodeId::new(task_identity)
            .map_err(|_| crate::turn::developer_error(&error("invalid logical task identity")))?;
        let binding = WorkingBinding::new(
            input.run_id,
            input.workspace_id,
            task,
            harness_role,
            input.conversation.revision(),
        );
        let root = input.trace_path.with_extension("context").join(role);
        Ok(Some(Self {
            root,
            workspace: input.workspace_root.clone(),
            trace: input.trace_path.clone(),
            binding,
            config,
            workspace_scope: protected.map_or_else(
                super::memory::environment::WorkspaceScope::default,
                |protected| super::memory::environment::WorkspaceScope {
                    direct: true, protected: protected.to_vec(),
                },
            ),
            runtime: input.command_runtime.clone(),
            conversation: Arc::clone(&input.conversation),
            cancellation: input.provider_cancellation.clone(),
        }))
    }

    fn open(self) -> Result<LocalContextHandle, ProductRunnerError> {
        let mut memory = LocalMemory::load_scoped_cancellable(
            &self.root, &self.workspace, &self.trace, self.binding, self.config,
            self.workspace_scope, self.cancellation,
        ).map_err(|error| crate::turn::developer_error(&error))?;
        memory.compactor_runtime = Some(self.runtime);
        Ok(LocalContextHandle {
            inner: Arc::new(Mutex::new(memory)),
            conversation: self.conversation,
            session_directory: self.root.join("provider-sessions"),
        })
    }
}

impl DeveloperContextPort for LocalContextHandle {
    fn compaction_owner(&self) -> DeveloperCompactionOwner {
        DeveloperCompactionOwner::LocalContext
    }

    fn local_session_directory(&self) -> Option<std::path::PathBuf> {
        Some(self.session_directory.clone())
    }
    fn resume(
        &mut self,
        request: &DeveloperLoopRequest,
        initial_messages: &[Message],
    ) -> Result<Option<DeveloperContextResume>, DeveloperLoopError> {
        self.lock()?.resume(request, initial_messages)
    }
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
                let pending_batch = memory
                    .segment_continuation
                    .as_mut()
                    .and_then(|segment| segment.pending_batch.take());
                let result = (|| {
                    memory.compact_locally()?;
                    if (memory.config.checkpoint_every_completed_batch
                        || memory.segment_continuation.is_some())
                        && let Some(profile) = memory.profile.clone()
                    {
                        let tools = memory.tools.clone();
                        let messages = memory.prepare_view(&profile, &tools)?;
                        memory.publish(&messages)?;
                    }
                    Ok(())
                })();
                if let Err(failure) = result {
                    if let Some(segment) = memory.segment_continuation.as_mut() {
                        segment.pending_batch = pending_batch;
                    }
                    return Err(failure);
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
        self.lock()?.prepare_view_with_governing(
            request.profile,
            request.tools,
            Some(request.invocation_policy),
            request.governing_input,
        )
    }
    fn checkpoint(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.lock()?.publish(messages)
    }

    fn schedule_segment(
        &mut self,
        next_segment: u64,
        progress: DeveloperLoopProgress,
        pending_calls: &[CompletedToolCall],
    ) -> Result<bool, DeveloperLoopError> {
        let mut memory = self.lock()?;
        if next_segment == 0 {
            return Err(error("cannot checkpoint a zero developer segment"));
        }
        let previous = memory.segment_continuation.clone();
        let expected = match previous.as_ref() {
            Some(segment) => segment
                .segment_sequence
                .checked_add(1)
                .ok_or_else(|| error("developer segment sequence overflow"))?,
            None => 1,
        };
        if next_segment != expected {
            return Err(error("noncontiguous developer segment continuation"));
        }
        if previous.as_ref().is_some_and(|segment| {
            segment.pending_batch.is_some()
                || progress.model_turns() < segment.progress.model_turns
                || progress.tool_calls() < segment.progress.tool_calls
                || progress.compactions() < segment.progress.compactions
                || progress.retries() < segment.progress.retries
        }) {
            return Err(error("developer segment progress regressed or retained a pending batch"));
        }
        let proposed = memory
            .transcript
            .pending
            .iter()
            .filter(|pending| pending.handle.is_none())
            .collect::<Vec<_>>();
        let pending_batch = if pending_calls.is_empty() {
            if !proposed.is_empty() {
                return Err(error("segment checkpoint omitted pending provider calls"));
            }
            None
        } else {
            let identities = pending_calls.iter().map(call_identity).collect::<Vec<_>>();
            if identities
                .iter()
                .enumerate()
                .any(|(index, identity)| identities[..index].contains(identity))
                || proposed.len() != identities.len()
                || proposed.iter().any(|pending| {
                    pending.state != PendingState::Proposed
                        || !identities.contains(&pending.call)
                })
            {
                return Err(error("segment checkpoint pending call identity mismatch"));
            }
            let assistant_source = proposed
                .first()
                .map(|pending| pending.source)
                .ok_or_else(|| error("segment checkpoint has no pending assistant source"))?;
            if proposed.iter().any(|pending| pending.source != assistant_source) {
                return Err(error("segment checkpoint spans multiple provider batches"));
            }
            Some(SegmentPendingBatch { assistant_source, calls: identities })
        };
        let invocation = memory.transcript.invocation;
        let request_prefix = memory.transcript.request_prefix.clone();
        let protocol_limits_sha256 = super::reentry::protocol_limits_sha256()?;
        memory.segment_continuation = Some(SegmentContinuation {
            schema_version: SEGMENT_CONTINUATION_SCHEMA_VERSION,
            invocation,
            request_prefix,
            segment_sequence: next_segment,
            protocol_limits_sha256,
            progress: SegmentProgress {
                model_turns: progress.model_turns(),
                tool_calls: progress.tool_calls(),
                compactions: progress.compactions(),
                retries: progress.retries(),
            },
            pending_batch,
        });
        let result = (|| {
            memory.compact_locally()?;
            let profile = memory
                .profile
                .clone()
                .ok_or_else(|| error("segment continuation has no provider profile"))?;
            let tools = memory.tools.clone();
            let messages = memory.prepare_view(&profile, &tools)?;
            memory.publish(&messages)
        })();
        if let Err(failure) = result {
            memory.segment_continuation = previous;
            return Err(failure);
        }
        Ok(true)
    }

    fn complete_invocation(&mut self) -> Result<(), DeveloperLoopError> {
        self.lock()?.complete_segment_continuation()
    }
}

fn call_identity(call: &CompletedToolCall) -> CallIdentity {
    CallIdentity {
        id: call.id().expose_for_wire().to_owned(),
        name: call.name().as_str().to_owned(),
        arguments_digest: peritus_codec::sha256(call.arguments().canonical_bytes()).into_bytes(),
    }
}
