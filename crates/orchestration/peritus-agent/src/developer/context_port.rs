//! Host-owned local context boundary for the production developer loop.

use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, Message, ProtocolLimits, ProviderProfile,
    ToolDefinition,
};
use serde_json::Value;

use super::context_encoding::estimated_request_tokens;
use super::{DeveloperLoopError, DeveloperLoopRequest, DeveloperToolObservation};

/// Estimates the complete input request using the same accounting as the production loop.
/// Output capacity is not subtracted from an input-only provider ceiling.
#[must_use]
pub fn estimate_developer_request_tokens(messages: &[Message], tools: &[ToolDefinition]) -> u64 {
    estimated_request_tokens(messages, tools)
}

/// Composable input estimates using the production loop's exact accounting weights.
/// Retaining a message contribution allows selection without reloading its source bytes.
#[derive(Clone, Copy, Default)]
pub struct DeveloperContextEstimate(u64);

impl DeveloperContextEstimate {
    /// Complete request framing, including tool definitions and fixed overhead.
    #[must_use]
    pub fn request(messages: &[Message], tools: &[ToolDefinition]) -> Self {
        Self(super::context_encoding::estimated_request_bytes(messages, tools))
    }
    /// Only these messages, without duplicate request or tool-definition overhead.
    #[must_use]
    pub fn messages(messages: &[Message]) -> Self {
        Self(super::context_encoding::estimated_message_bytes(messages))
    }
    /// Appends another retained contribution with saturating accounting.
    #[must_use]
    pub const fn with(self, additional: Self) -> Self {
        Self(self.0.saturating_add(additional.0))
    }
    /// Replaces one previously included message contribution without loading other messages.
    #[must_use]
    pub const fn replacing(self, previous: Self, replacement: Self) -> Self {
        Self(self.0.saturating_sub(previous.0).saturating_add(replacement.0))
    }
    /// Applies the production token estimate after all contributions have been combined.
    #[must_use]
    pub const fn tokens(self) -> u64 {
        super::context_encoding::estimate_u64(self.0)
    }
}

/// Inputs to local selection. The host must preserve current policy and protocol obligations.
pub struct DeveloperContextAssembly<'a> {
    /// The receiving adapter's separate transport envelope, when one exists.
    pub provider: &'a dyn peritus_provider_core::ModelProvider,
    /// Exact replaceable user-governing projection. Preserve it as a user message, not system
    /// authority or optional archive evidence. Unincorporated revisions must not accumulate.
    pub governing_input: Option<&'a Message>,
    /// Stable current system policy, preserved exactly at the head of the request.
    pub invocation_policy: &'a Message,
    /// Authoritative current executor guidance, separate from the reusable policy/evidence
    /// prefix. Preserve it exactly as a developer message; never recover it from old memory.
    pub invocation_state: Option<&'a Message>,
    /// Current view and the completed events appended since its installation.
    pub messages: &'a [Message],
    /// Complete tool definitions included in request accounting.
    pub tools: &'a [ToolDefinition],
    /// Receiving provider's protocol and input capacity; not a memory namespace key.
    pub profile: &'a ProviderProfile,
}

pub(super) struct DeveloperContextPreparation<'a> {
    pub(super) provider: &'a dyn peritus_provider_core::ModelProvider,
    pub(super) tools: &'a [ToolDefinition],
    pub(super) profile: &'a ProviderProfile,
    pub(super) invocation_policy: &'a Message,
    pub(super) invocation_state: Option<&'a Message>,
    pub(super) governing_input: Option<&'a Message>,
}

/// Authorized visible events supplied to local memory in execution order.
///
/// Observing a proposal does not authorize it or establish that it executed. Tool output is
/// non-authoritative evidence even when it contains instruction-like text.
pub enum DeveloperContextEvent<'a> {
    /// Exact visible message, including proposed calls, result views, and host corrections.
    Message(&'a Message),
    /// Replaceable host feedback or media evidence; never a literal user instruction.
    HostObservation(&'a Message),
    /// Complete tool output, after trace persistence and before model-visible limiting.
    ToolObservation {
        /// Associated proposal; its identifier is scoped to the current invocation.
        call: &'a CompletedToolCall,
        /// Original authorized output, which may exceed the prompt view's output limit.
        observation: &'a DeveloperToolObservation,
    },
    /// A complete tool batch, including any host progress correction, has been ingested.
    BatchCompleted,
}

/// Local context effects supplied by the product host for one prebound logical task and role.
///
/// The host owns this port across invocations and failures. Implementations must enforce scope,
/// persist accepted observations before returning, and use only local storage and computation.
/// Invocation prefixes and provider profiles annotate events, never locate the task lineage.
/// The port grants no tool authority or grounding credit. D0 executes only fresh provider calls;
/// pending operations in recovered context remain the host's effect-recovery responsibility.
///
/// This seam does not itself implement durable storage, working-state reduction, or retrieval.
/// Those policies belong in C6 with effects supplied by the product host through C0 facilities.
pub trait DeveloperContextPort: Send {
    /// Binds the final complete request, including options and tool choice, before admission.
    /// The default preserves existing embedders; this does not grant provider authority.
    ///
    /// # Errors
    /// Returns a local binding or persistence failure before provider dispatch.
    fn bind_request(
        &mut self,
        _request: &peritus_model_protocol::ModelRequest,
    ) -> Result<(), DeveloperLoopError> {
        Ok(())
    }
    /// Reopens the bound lineage and durably records the current invocation's exact inputs.
    ///
    /// `initial_messages` contains the current policy and user prompt, including attachments.
    /// Historical instructions must not replace these current inputs.
    ///
    /// # Errors
    /// Rejects incompatible bindings, unresolved recovery, or failed persistence.
    fn open(
        &mut self,
        request: &DeveloperLoopRequest,
        initial_messages: &[Message],
    ) -> Result<(), DeveloperLoopError>;

    /// Commits an event before D0 admits it to further work or model context.
    ///
    /// # Errors
    /// Returns a redaction-safe ingestion or persistence failure; D0 stops immediately.
    fn observe(&mut self, event: DeveloperContextEvent<'_>) -> Result<(), DeveloperLoopError>;

    /// Returns bounded host metadata for an already durably ingested original observation.
    ///
    /// The loop adds this after output limiting, overwriting any forged `local_context` field.
    /// Default ports need not expose retrieval handles. Metadata is never archived as the original.
    ///
    /// # Errors
    /// Rejects unresolved or wrongly scoped sources; a failure stops forward progress.
    fn source_reference(
        &self,
        _call: &CompletedToolCall,
    ) -> Result<Option<CanonicalJson>, DeveloperLoopError> {
        Ok(None)
    }

    /// Prepares an immutable, scope-checked view from committed state and source observations.
    ///
    /// This must not publish the candidate. D0 validates its complete request budget before
    /// calling `checkpoint`. A failure never selects legacy or remote compaction.
    ///
    /// # Errors
    /// Rejects unavailable evidence, stale bindings, unresolved exchanges, or pinned capacity.
    fn assemble(
        &mut self,
        request: DeveloperContextAssembly<'_>,
    ) -> Result<Vec<Message>, DeveloperLoopError>;

    /// Persists the exact prepared view and its reconstruction manifest before publication.
    ///
    /// Implementations validate generation and artifact reachability before atomically
    /// publishing. A failure preserves the previously committed checkpoint.
    ///
    /// # Errors
    /// Rejects stale candidates, invalid manifests, and persistence failures.
    fn checkpoint(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError>;
}

pub(super) struct ContextSession<'a>(pub(super) Option<&'a mut dyn DeveloperContextPort>);

impl ContextSession<'_> {
    pub(super) fn bind_request(
        &mut self,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<(), DeveloperLoopError> {
        if let Some(port) = &mut self.0 {
            port.bind_request(request)?;
        }
        Ok(())
    }
    pub(super) fn annotate(
        &self,
        call: &CompletedToolCall,
        output: CanonicalJson,
    ) -> Result<CanonicalJson, DeveloperLoopError> {
        let Some(port) = &self.0 else {
            return Ok(output);
        };
        let Some(metadata) = port.source_reference(call)? else {
            return Ok(output);
        };
        if metadata.canonical_bytes().len() > 1024 {
            return Err(DeveloperLoopError::Context(
                "local source metadata exceeds bound".to_owned(),
            ));
        }
        let mut value: Value = serde_json::from_slice(output.canonical_bytes())
            .map_err(|_| DeveloperLoopError::Context("invalid tool output JSON".to_owned()))?;
        let metadata = serde_json::from_slice(metadata.canonical_bytes())
            .map_err(|_| DeveloperLoopError::Context("invalid local source metadata".to_owned()))?;
        if let Some(object) = value.as_object_mut() {
            object.insert("local_context".to_owned(), metadata);
        } else {
            value = Value::from_iter([("output", value), ("local_context", metadata)]);
        }
        Ok(CanonicalJson::parse(&value.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))?)
    }

    pub(super) fn is_local(&self) -> bool {
        self.0.is_some()
    }

    pub(super) fn open(
        &mut self,
        request: &DeveloperLoopRequest,
        messages: &[Message],
    ) -> Result<(), DeveloperLoopError> {
        if let Some(port) = &mut self.0 {
            port.open(request, messages)?;
        }
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        event: DeveloperContextEvent<'_>,
    ) -> Result<(), DeveloperLoopError> {
        if let Some(port) = &mut self.0 {
            port.observe(event)?;
        }
        Ok(())
    }

    pub(super) fn append(
        &mut self,
        messages: &mut Vec<Message>,
        message: Message,
    ) -> Result<(), DeveloperLoopError> {
        self.observe(DeveloperContextEvent::Message(&message))?;
        messages.push(message);
        Ok(())
    }

    pub(super) fn append_host(
        &mut self,
        messages: &mut Vec<Message>,
        message: Message,
    ) -> Result<(), DeveloperLoopError> {
        self.observe(DeveloperContextEvent::HostObservation(&message))?;
        messages.push(message);
        Ok(())
    }

    pub(super) fn prepare(
        &mut self,
        messages: &mut Vec<Message>,
        preparation: &DeveloperContextPreparation<'_>,
    ) -> Result<bool, DeveloperLoopError> {
        if let Some(port) = &mut self.0 {
            let prior = estimated_request_tokens(messages, preparation.tools);
            let prior_units = transcript_units(messages);
            let candidate = port.assemble(DeveloperContextAssembly {
                provider: preparation.provider,
                governing_input: preparation.governing_input,
                invocation_policy: preparation.invocation_policy,
                invocation_state: preparation.invocation_state,
                messages,
                tools: preparation.tools,
                profile: preparation.profile,
            })?;
            if candidate.first() != Some(preparation.invocation_policy) {
                return Err(DeveloperLoopError::Context(
                    "local view did not preserve current host invocation policy".to_owned(),
                ));
            }
            if preparation.invocation_state.is_some_and(|state| !candidate.contains(state)) {
                return Err(DeveloperLoopError::Context(
                    "local view did not preserve current executor state".to_owned(),
                ));
            }
            if preparation.governing_input.is_some_and(|input| !candidate.contains(input)) {
                return Err(DeveloperLoopError::Context(
                    "local view did not preserve current governing user input".to_owned(),
                ));
            }
            let estimated = estimated_request_tokens(&candidate, preparation.tools);
            port.checkpoint(&candidate)?;
            let reduced_units = transcript_units(&candidate) < prior_units;
            *messages = candidate;
            // A smaller policy or a rebuilt memory preview is not another compaction.
            // Count displacement of actual transcript units, not every local view rebuild.
            return Ok(estimated < prior && reduced_units);
        }
        Ok(false)
    }
}

fn transcript_units(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| {
            usize::from(message.role() == peritus_model_protocol::Role::Assistant)
                + message
                    .content()
                    .iter()
                    .filter(|block| {
                        matches!(block, peritus_model_protocol::ContentBlock::ToolResult(_))
                    })
                    .count()
        })
        .sum()
}
