//! Host-owned local context boundary for the production developer loop.

use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, ContentBlock, JsonBounds, Message, ProtocolLimits,
    ProviderProfile, Role, ToolDefinition,
};
use serde_json::Value;
use std::collections::BTreeSet;

use super::context_encoding::estimated_request_tokens;
use super::{
    DeveloperCompactionOwner, DeveloperInteraction, DeveloperLoopError, DeveloperLoopRequest,
    DeveloperLoopProgress, DeveloperToolObservation,
};

/// Estimates the complete input request using the same accounting as the production loop.
/// Output capacity is not subtracted from an input-only provider ceiling.
#[must_use]
pub fn estimate_developer_request_tokens(messages: &[Message], tools: &[ToolDefinition]) -> u64 {
    estimated_request_tokens(messages, tools)
}

/// Inputs to local selection. The host must preserve current policy and protocol obligations.
pub struct DeveloperContextAssembly<'a> {
    /// Exact replaceable user-governing projection. Preserve it as a user message, not system
    /// authority or optional archive evidence. Unincorporated revisions must not accumulate.
    pub governing_input: Option<&'a Message>,
    /// Current system policy plus live invocation position and executor prerequisite.
    /// Replace the initial system message with this exact message before budgeting. Never
    /// recover its lifecycle fields from an earlier checkpoint or model-authored memory.
    pub invocation_policy: &'a Message,
    /// Current view and the completed events appended since its installation.
    pub messages: &'a [Message],
    /// Complete tool definitions included in request accounting.
    pub tools: &'a [ToolDefinition],
    /// Receiving provider's protocol and input capacity; not a memory namespace key.
    pub profile: &'a ProviderProfile,
}

/// Exact local invocation state authorized for a pending definitely-unaccepted retry.
pub struct DeveloperContextResume {
    turn: u16,
    system: String,
    prompt: String,
    attachments: Vec<peritus_model_protocol::MediaInput>,
    messages: Vec<Message>,
    tool_calls: u32,
    retries: u64,
    segment_continuation: bool,
    progress: DeveloperLoopProgress,
    pending_exchange: Vec<Message>,
    pending_calls: Vec<CompletedToolCall>,
    pending_first_sequence: u32,
}

impl DeveloperContextResume {
    /// Creates checked continuation state from a host-validated durable invocation.
    ///
    /// # Errors
    /// Rejects a zero turn or an empty provider request view.
    pub fn new(
        turn: u16,
        initial_messages: &[Message],
        messages: Vec<Message>,
        tool_calls: u32,
        retries: u64,
    ) -> Result<Self, DeveloperLoopError> {
        Self::new_with_kind(
            turn,
            initial_messages,
            messages,
            tool_calls,
            retries,
            false,
            DeveloperLoopProgress::default(),
            Vec::new(),
            Vec::new(),
            1,
        )
    }

    /// Creates checked state for a new physical segment of the same durable invocation.
    ///
    /// # Errors
    /// Rejects a zero turn or an empty provider request view.
    pub fn new_segment(
        turn: u16,
        initial_messages: &[Message],
        messages: Vec<Message>,
        tool_calls: u32,
        retries: u64,
    ) -> Result<Self, DeveloperLoopError> {
        Self::new_with_kind(
            turn,
            initial_messages,
            messages,
            tool_calls,
            retries,
            true,
            DeveloperLoopProgress::default(),
            Vec::new(),
            Vec::new(),
            1,
        )
    }

    /// Creates checked state for a physical segment with exact cumulative progress and an
    /// optional provider-generated tool batch that has not completed admission.
    ///
    /// `pending_exchange` is exact durable replay beginning with the batch's assistant message
    /// and any already committed tool-result prefix. `pending_calls` is the unfinished suffix.
    ///
    /// # Errors
    /// Rejects malformed replay, zero sequence identities, or invalid ordinary resume state.
    pub fn new_segment_with_progress(
        turn: u16,
        initial_messages: &[Message],
        messages: Vec<Message>,
        tool_calls: u32,
        retries: u64,
        progress: DeveloperLoopProgress,
        pending_exchange: Vec<Message>,
        pending_calls: Vec<CompletedToolCall>,
        pending_first_sequence: u32,
    ) -> Result<Self, DeveloperLoopError> {
        if pending_first_sequence == 0
            || pending_exchange
                .first()
                .is_some_and(|message| message.role() != Role::Assistant)
            || (pending_exchange.is_empty() && !pending_calls.is_empty())
        {
            return Err(DeveloperLoopError::Context(
                "invalid durable pending tool-batch replay".to_owned(),
            ));
        }
        Self::new_with_kind(
            turn,
            initial_messages,
            messages,
            tool_calls,
            retries,
            true,
            progress,
            pending_exchange,
            pending_calls,
            pending_first_sequence,
        )
    }

    fn new_with_kind(
        turn: u16,
        initial_messages: &[Message],
        messages: Vec<Message>,
        tool_calls: u32,
        retries: u64,
        segment_continuation: bool,
        progress: DeveloperLoopProgress,
        pending_exchange: Vec<Message>,
        pending_calls: Vec<CompletedToolCall>,
        pending_first_sequence: u32,
    ) -> Result<Self, DeveloperLoopError> {
        let [system, user] = initial_messages else {
            return Err(DeveloperLoopError::Context(
                "invalid durable invocation resume state".to_owned(),
            ));
        };
        let [ContentBlock::Text(system_text)] = system.content() else {
            return Err(DeveloperLoopError::Context(
                "durable invocation system input changed shape".to_owned(),
            ));
        };
        let Some((ContentBlock::Text(prompt_text), attachment_blocks)) =
            user.content().split_first()
        else {
            return Err(DeveloperLoopError::Context(
                "durable invocation user input changed shape".to_owned(),
            ));
        };
        if turn == 0
            || system.role() != peritus_model_protocol::Role::System
            || user.role() != peritus_model_protocol::Role::User
            || messages.is_empty()
        {
            return Err(DeveloperLoopError::Context(
                "invalid durable invocation resume state".to_owned(),
            ));
        }
        let attachments = attachment_blocks
            .iter()
            .map(|block| match block {
                ContentBlock::Image(media) => Ok(media.clone()),
                _ => Err(DeveloperLoopError::Context(
                    "durable invocation attachment changed shape".to_owned(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            turn,
            system: system_text.expose_for_wire().to_owned(),
            prompt: prompt_text.expose_for_wire().to_owned(),
            attachments,
            messages,
            tool_calls,
            retries,
            segment_continuation,
            progress,
            pending_exchange,
            pending_calls,
            pending_first_sequence,
        })
    }

    pub(super) const fn turn(&self) -> u16 {
        self.turn
    }

    pub(super) const fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub(super) const fn retries(&self) -> u64 {
        self.retries
    }

    pub(super) const fn is_segment_continuation(&self) -> bool {
        self.segment_continuation
    }

    pub(super) const fn progress(&self) -> DeveloperLoopProgress {
        self.progress
    }

    pub(super) fn into_request_state(
        self,
    ) -> (
        String,
        String,
        Vec<peritus_model_protocol::MediaInput>,
        Vec<Message>,
        Vec<Message>,
        Vec<CompletedToolCall>,
        u32,
    ) {
        (
            self.system,
            self.prompt,
            self.attachments,
            self.messages,
            self.pending_exchange,
            self.pending_calls,
            self.pending_first_sequence,
        )
    }
}

/// Authorized visible events supplied to local memory in execution order.
///
/// Observing a proposal does not authorize it or establish that it executed. Tool output is
/// non-authoritative evidence even when it contains instruction-like text.
pub enum DeveloperContextEvent<'a> {
    /// Exact visible message, including proposed calls, result views, and host corrections.
    Message(&'a Message),
    /// Exact tool effect whose permission admission completed and whose executor is about to be
    /// entered. Persistence of this boundary makes a missing result an unresolved effect; it does
    /// not claim that the executor accepted or completed the effect.
    ToolEffectUncertain {
        /// Exact provider proposal about to cross the executor boundary.
        call: &'a CompletedToolCall,
        /// One-based contiguous tool identity within the complete logical invocation.
        sequence: u32,
    },
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
    /// Identifies the owner implemented by this port.
    ///
    /// Context ports own local selection and checkpoint publication. Returning a legacy owner is
    /// rejected before the invocation is opened.
    fn compaction_owner(&self) -> DeveloperCompactionOwner {
        DeveloperCompactionOwner::LocalContext
    }

    /// Native runtime persistence directory owned by this exact task and role lineage.
    /// It is independent of invocation prefixes and provider response cursors.
    fn local_session_directory(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// Returns the exact published provider view for a pending safe retry, when one exists.
    ///
    /// The default starts a new invocation. Implementations may resume only a durable request
    /// known not to have been accepted; ambiguous or settled requests must fail closed.
    ///
    /// # Errors
    /// Rejects changed inputs, conflicting identity, unresolved acceptance, or corrupt state.
    fn resume(
        &mut self,
        _request: &DeveloperLoopRequest,
        _initial_messages: &[Message],
    ) -> Result<Option<DeveloperContextResume>, DeveloperLoopError> {
        Ok(None)
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

    /// Commits the exact next physical segment of this durable logical invocation.
    ///
    /// Returning `false` means this port cannot own durable segmented continuation, so the loop
    /// reports an explicit segment exhaustion instead.
    ///
    /// # Errors
    /// Rejects an incomplete tool exchange, conflicting segment identity, or failed publication.
    fn schedule_segment(
        &mut self,
        _next_segment: u64,
        _progress: DeveloperLoopProgress,
        _pending_calls: &[CompletedToolCall],
    ) -> Result<bool, DeveloperLoopError> {
        Ok(false)
    }

    /// Settles a previously scheduled segment after the logical invocation reaches a terminal.
    ///
    /// # Errors
    /// Returns a durable settlement failure before the terminal may leave the loop.
    fn complete_invocation(&mut self) -> Result<(), DeveloperLoopError> {
        Ok(())
    }
}

pub(super) struct ContextSession<'a>(pub(super) Option<&'a mut dyn DeveloperContextPort>);

impl ContextSession<'_> {
    pub(super) fn negotiate_compaction(
        &self,
        interaction: Option<&dyn DeveloperInteraction>,
    ) -> Result<DeveloperCompactionOwner, DeveloperLoopError> {
        let requested = interaction.map(|port| port.compaction_owner());
        match (self.0.as_ref(), requested) {
            (Some(port), Some(requested)) => {
                let supplied = port.compaction_owner();
                if supplied != DeveloperCompactionOwner::LocalContext {
                    return Err(DeveloperLoopError::Context(
                        "context port declared a legacy compaction owner".to_owned(),
                    ));
                }
                if requested != supplied {
                    return Err(DeveloperLoopError::RecoveryRequired(
                        "host compaction ownership changed before invocation reentry".to_owned(),
                    ));
                }
                Ok(supplied)
            }
            (Some(port), None) => {
                let supplied = port.compaction_owner();
                if supplied == DeveloperCompactionOwner::LocalContext {
                    Ok(supplied)
                } else {
                    Err(DeveloperLoopError::Context(
                        "context port declared a legacy compaction owner".to_owned(),
                    ))
                }
            }
            (None, Some(DeveloperCompactionOwner::LocalContext)) => {
                Err(DeveloperLoopError::RecoveryRequired(
                    "the retained local compaction owner is unavailable for this invocation"
                        .to_owned(),
                ))
            }
            (None, Some(owner)) => Ok(owner),
            (None, None) => Ok(DeveloperCompactionOwner::LEGACY),
        }
    }

    pub(super) fn resume(
        &mut self,
        request: &DeveloperLoopRequest,
        messages: &[Message],
    ) -> Result<Option<DeveloperContextResume>, DeveloperLoopError> {
        self.0.as_mut().map_or(Ok(None), |port| port.resume(request, messages))
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

    pub(super) fn local_session_directory(&self) -> Option<std::path::PathBuf> {
        self.0.as_ref().and_then(|port| port.local_session_directory())
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

    pub(super) fn prepare(
        &mut self,
        messages: &mut Vec<Message>,
        tools: &[ToolDefinition],
        profile: &ProviderProfile,
        invocation_policy: &Message,
        governing_input: Option<&Message>,
    ) -> Result<bool, DeveloperLoopError> {
        if let Some(port) = &mut self.0 {
            let prior = estimated_request_tokens(messages, tools);
            let candidate = port.assemble(DeveloperContextAssembly {
                governing_input,
                invocation_policy,
                messages,
                tools,
                profile,
            })?;
            if candidate.first() != Some(invocation_policy) {
                return Err(DeveloperLoopError::Context(
                    "local view did not preserve current host invocation policy".to_owned(),
                ));
            }
            if governing_input.is_some_and(|input| !candidate.contains(input)) {
                return Err(DeveloperLoopError::Context(
                    "local view did not preserve current governing user input".to_owned(),
                ));
            }
            validate_complete_tool_protocol(&candidate)?;
            let estimated = estimated_request_tokens(&candidate, tools);
            let capacity = profile.limits().max_input_tokens();
            if estimated > capacity {
                return Err(DeveloperLoopError::Context(format!(
                    "local view estimated input tokens {estimated} exceed provider limit {capacity}"
                )));
            }
            port.checkpoint(&candidate)?;
            *messages = candidate;
            return Ok(estimated < prior);
        }
        Ok(false)
    }

    pub(super) fn schedule_segment(
        &mut self,
        next_segment: u64,
        progress: DeveloperLoopProgress,
        pending_calls: &[CompletedToolCall],
    ) -> Result<bool, DeveloperLoopError> {
        self.0
            .as_mut()
            .map_or(Ok(false), |port| {
                port.schedule_segment(next_segment, progress, pending_calls)
            })
    }

    pub(super) fn complete_invocation(&mut self) -> Result<(), DeveloperLoopError> {
        if let Some(port) = &mut self.0 {
            port.complete_invocation()?;
        }
        Ok(())
    }
}

fn validate_complete_tool_protocol(messages: &[Message]) -> Result<(), DeveloperLoopError> {
    let mut pending = BTreeSet::new();
    for message in messages {
        match message.role() {
            Role::Assistant => {
                if !pending.is_empty() {
                    return Err(DeveloperLoopError::Context(
                        "local view split a pending tool protocol exchange".to_owned(),
                    ));
                }
                for block in message.content() {
                    if let ContentBlock::ToolCall(call) = block
                        && !pending.insert(call.id().expose_for_wire().to_owned())
                    {
                        return Err(DeveloperLoopError::Context(
                            "local view duplicated a pending tool call".to_owned(),
                        ));
                    }
                }
            }
            Role::Tool => {
                for block in message.content() {
                    if let ContentBlock::ToolResult(result) = block
                        && !pending.remove(result.call_id().expose_for_wire())
                    {
                        return Err(DeveloperLoopError::Context(
                            "local view retained a tool result without its exact call".to_owned(),
                        ));
                    }
                }
            }
            Role::System | Role::Developer | Role::User if !pending.is_empty() => {
                return Err(DeveloperLoopError::Context(
                    "local view split a pending tool protocol exchange".to_owned(),
                ));
            }
            Role::System | Role::Developer | Role::User => {}
        }
    }
    if pending.is_empty() {
        Ok(())
    } else {
        Err(DeveloperLoopError::Context(
            "local view retained tool calls without their complete results".to_owned(),
        ))
    }
}
