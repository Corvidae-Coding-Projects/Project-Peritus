//! Replayable agent-turn aggregate state.

use crate::{
    ActivePhase, AgentBinding, AgentCounters, AgentLimitDimension, AgentLimits, AgentPhase,
    CompletionProposal, ContextRecord, ModelCallId, SafeText, TerminalKind, ToolBatch,
};
use peritus_types::{EventId, EventSequence, RevisionNumber, Sha256Digest};

/// Stable failure category retained in terminal state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AgentFailureKind {
    Provider,
    Tool,
    Context,
    Protocol,
    Exhausted(AgentLimitDimension),
    Indeterminate,
}

/// Bounded terminal failure record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentFailure {
    kind: AgentFailureKind,
    detail: SafeText,
}

impl AgentFailure {
    #[must_use]
    pub const fn new(kind: AgentFailureKind, detail: SafeText) -> Self {
        Self { kind, detail }
    }
    #[must_use]
    pub const fn kind(&self) -> AgentFailureKind {
        self.kind
    }
    #[must_use]
    pub const fn detail(&self) -> &SafeText {
        &self.detail
    }
}

/// Current model interaction projection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent provider terminal facts are replayed exactly"
)]
pub struct ModelState {
    call_id: Option<ModelCallId>,
    request_digest: Option<Sha256Digest>,
    response_digest: Option<Sha256Digest>,
    stream_digest: Option<Sha256Digest>,
    cursor: u64,
    in_flight: bool,
    normal_terminal: bool,
    incomplete_items: bool,
    usage_settled: bool,
    retry_count: u32,
    retry_pending: bool,
    resume_exact: bool,
}

impl ModelState {
    #[must_use]
    pub const fn call_id(self) -> Option<ModelCallId> {
        self.call_id
    }
    #[must_use]
    pub const fn request_digest(self) -> Option<Sha256Digest> {
        self.request_digest
    }
    #[must_use]
    pub const fn response_digest(self) -> Option<Sha256Digest> {
        self.response_digest
    }
    #[must_use]
    pub const fn stream_digest(self) -> Option<Sha256Digest> {
        self.stream_digest
    }
    #[must_use]
    pub const fn cursor(self) -> u64 {
        self.cursor
    }
    #[must_use]
    pub const fn in_flight(self) -> bool {
        self.in_flight
    }
    #[must_use]
    pub const fn normal_terminal(self) -> bool {
        self.normal_terminal
    }
    #[must_use]
    pub const fn incomplete_items(self) -> bool {
        self.incomplete_items
    }
    #[must_use]
    pub const fn usage_settled(self) -> bool {
        self.usage_settled
    }
    #[must_use]
    pub const fn retry_count(self) -> u32 {
        self.retry_count
    }
    #[must_use]
    pub const fn retry_pending(self) -> bool {
        self.retry_pending
    }
    #[must_use]
    pub const fn resume_exact(self) -> bool {
        self.resume_exact
    }
}

/// Complete replayable state for one immutable turn binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTurnState {
    pub(crate) binding: AgentBinding,
    pub(crate) limits: AgentLimits,
    pub(crate) counters: AgentCounters,
    pub(crate) phase: AgentPhase,
    pub(crate) paused_from: Option<ActivePhase>,
    pub(crate) logical_revision: RevisionNumber,
    pub(crate) sequence: EventSequence,
    pub(crate) last_event_id: EventId,
    pub(crate) state_digest: Sha256Digest,
    pub(crate) context: Option<ContextRecord>,
    pub(crate) model: ModelState,
    pub(crate) tools: Option<ToolBatch>,
    pub(crate) tool_transcript_digest: Option<Sha256Digest>,
    pub(crate) completion: Option<CompletionProposal>,
    pub(crate) failure: Option<AgentFailure>,
    pub(crate) unresolved_indeterminate: bool,
}

impl AgentTurnState {
    #[must_use]
    pub const fn binding(&self) -> &AgentBinding {
        &self.binding
    }
    #[must_use]
    pub const fn limits(&self) -> AgentLimits {
        self.limits
    }
    #[must_use]
    pub const fn counters(&self) -> AgentCounters {
        self.counters
    }
    #[must_use]
    pub const fn phase(&self) -> AgentPhase {
        self.phase
    }
    #[must_use]
    pub const fn paused_from(&self) -> Option<ActivePhase> {
        self.paused_from
    }
    #[must_use]
    pub const fn logical_revision(&self) -> RevisionNumber {
        self.logical_revision
    }
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    #[must_use]
    pub const fn context(&self) -> Option<&ContextRecord> {
        self.context.as_ref()
    }
    #[must_use]
    pub const fn model(&self) -> ModelState {
        self.model
    }
    #[must_use]
    pub const fn tools(&self) -> Option<&ToolBatch> {
        self.tools.as_ref()
    }
    #[must_use]
    pub const fn tool_transcript_digest(&self) -> Option<Sha256Digest> {
        self.tool_transcript_digest
    }
    #[must_use]
    pub const fn completion(&self) -> Option<&CompletionProposal> {
        self.completion.as_ref()
    }
    #[must_use]
    pub const fn failure(&self) -> Option<&AgentFailure> {
        self.failure.as_ref()
    }
    #[must_use]
    pub const fn has_unresolved_indeterminate(&self) -> bool {
        self.unresolved_indeterminate
    }
    #[must_use]
    pub const fn terminal_kind(&self) -> Option<TerminalKind> {
        if let AgentPhase::Terminal(kind) = self.phase { Some(kind) } else { None }
    }
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        crate::canonical::state_bytes(self)
    }
}

pub const fn set_model_started(
    state: &mut AgentTurnState,
    call_id: ModelCallId,
    request_digest: Sha256Digest,
) {
    if !state.model.resume_exact {
        state.model.cursor = 0;
    }
    state.model.call_id = Some(call_id);
    state.model.request_digest = Some(request_digest);
    state.model.response_digest = None;
    state.model.in_flight = true;
    state.model.normal_terminal = false;
    state.model.incomplete_items = false;
    state.model.usage_settled = false;
    state.model.retry_pending = false;
    state.model.resume_exact = false;
}

pub fn schedule_retry(state: &mut AgentTurnState, record: crate::ProviderRetryRecord) {
    let mut bytes = b"peritus.agent.provider-retry.v1\0".to_vec();
    if let Some(prior) = state.model.stream_digest {
        bytes.extend_from_slice(prior.as_bytes());
    }
    bytes.extend_from_slice(record.failure_digest().as_bytes());
    bytes.extend_from_slice(record.request_digest().as_bytes());
    match record.class() {
        crate::ProviderRetryClass::ExactResume { cursor } => {
            bytes.push(1);
            bytes.extend_from_slice(&cursor.to_be_bytes());
            state.model.resume_exact = true;
        }
        crate::ProviderRetryClass::SafeNewRequest => {
            bytes.push(2);
            state.model.resume_exact = false;
        }
    }
    state.model.stream_digest = Some(peritus_codec::sha256(&bytes));
    state.model.call_id = None;
    state.model.request_digest = Some(record.request_digest());
    state.model.response_digest = None;
    state.model.in_flight = false;
    state.model.retry_count += 1;
    state.model.retry_pending = true;
}

pub fn observe_provider(state: &mut AgentTurnState, record: &crate::ProviderEventRecord) {
    let mut bytes = b"peritus.agent.provider-stream.v1\0".to_vec();
    if let Some(prior) = state.model.stream_digest {
        bytes.extend_from_slice(prior.as_bytes());
    }
    bytes.extend_from_slice(&record.cursor().to_be_bytes());
    bytes.extend_from_slice(record.event_digest().as_bytes());
    bytes.extend_from_slice(&record.output_bytes().to_be_bytes());
    bytes.push(u8::from(record.duplicate()));
    state.model.stream_digest = Some(peritus_codec::sha256(&bytes));
    if !record.duplicate() {
        state.model.cursor = record.cursor();
    }
}

pub const fn set_model_terminal(state: &mut AgentTurnState, terminal: crate::ModelTerminalRecord) {
    state.model.response_digest = Some(terminal.response_digest());
    state.model.in_flight = false;
    state.model.normal_terminal = terminal.normal_terminal();
    state.model.incomplete_items = terminal.incomplete_items();
    state.model.usage_settled = terminal.usage_settled();
}
