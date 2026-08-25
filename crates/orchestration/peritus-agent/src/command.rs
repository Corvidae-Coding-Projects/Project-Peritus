//! Closed command vocabulary for one pure agent-turn transition.

use crate::{
    ActivePhase, AgentFailure, CompletionProposal, ModelCallId, ToolOrdinal, ToolProposal,
    ToolResultRecord,
};
use peritus_types::{CommandId, EventId, RevisionNumber, Sha256Digest};

/// Exact assembled context observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_field_names, reason = "each retained value is explicitly a digest")]
pub struct ContextRecord {
    render_digest: Sha256Digest,
    plan_digest: Sha256Digest,
    memory_digest: Sha256Digest,
    estimator_digest: Sha256Digest,
    compaction_digest: Option<Sha256Digest>,
}

impl ContextRecord {
    #[must_use]
    pub const fn new(
        render_digest: Sha256Digest,
        plan_digest: Sha256Digest,
        memory_digest: Sha256Digest,
        estimator_digest: Sha256Digest,
        compaction_digest: Option<Sha256Digest>,
    ) -> Self {
        Self { render_digest, plan_digest, memory_digest, estimator_digest, compaction_digest }
    }
    #[must_use]
    pub const fn render_digest(self) -> Sha256Digest {
        self.render_digest
    }
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest {
        self.plan_digest
    }
    #[must_use]
    pub const fn memory_digest(self) -> Sha256Digest {
        self.memory_digest
    }
    #[must_use]
    pub const fn estimator_digest(self) -> Sha256Digest {
        self.estimator_digest
    }
    #[must_use]
    pub const fn compaction_digest(self) -> Option<Sha256Digest> {
        self.compaction_digest
    }
}

/// Ordered provider stream observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEventRecord {
    cursor: u64,
    event_digest: Sha256Digest,
    output_bytes: u64,
    duplicate: bool,
    encoded_envelope: Vec<u8>,
}

impl ProviderEventRecord {
    /// Maximum canonical C5 envelope retained in one durable D0 event.
    pub const MAX_ENVELOPE_BYTES: usize = 8 * 1024 * 1024;

    /// Creates a synthetic provider observation without an envelope replay capsule.
    ///
    /// Production provider driving uses [`Self::with_envelope`]. This constructor remains useful
    /// for pure reducer/conformance cases that do not claim resumable C5 reconstruction.
    #[must_use]
    pub const fn new(
        cursor: u64,
        event_digest: Sha256Digest,
        output_bytes: u64,
        duplicate: bool,
    ) -> Self {
        Self { cursor, event_digest, output_bytes, duplicate, encoded_envelope: Vec::new() }
    }

    /// Creates a provider observation with its exact bounded canonical C5 envelope.
    ///
    /// # Errors
    ///
    /// Rejects empty/excessive bytes or a digest that does not match the canonical envelope.
    pub fn with_envelope(
        cursor: u64,
        event_digest: Sha256Digest,
        output_bytes: u64,
        duplicate: bool,
        encoded_envelope: Vec<u8>,
    ) -> Result<Self, crate::AgentRejection> {
        if encoded_envelope.is_empty()
            || encoded_envelope.len() > Self::MAX_ENVELOPE_BYTES
            || peritus_codec::sha256(&encoded_envelope) != event_digest
        {
            return Err(crate::AgentRejection::new(
                crate::AgentErrorCode::InvalidCommand,
                crate::AgentOperation::Reduce,
                crate::AgentRecovery::CorrectRequest,
                "provider envelope capsule is empty, excessive, or digest-mismatched",
            ));
        }
        Ok(Self { cursor, event_digest, output_bytes, duplicate, encoded_envelope })
    }
    #[must_use]
    pub const fn cursor(&self) -> u64 {
        self.cursor
    }
    #[must_use]
    pub const fn event_digest(&self) -> Sha256Digest {
        self.event_digest
    }
    #[must_use]
    pub const fn output_bytes(&self) -> u64 {
        self.output_bytes
    }
    #[must_use]
    pub const fn duplicate(&self) -> bool {
        self.duplicate
    }
    /// Borrows the exact canonical C5 envelope, empty only for synthetic pure-domain cases.
    #[must_use]
    pub fn encoded_envelope(&self) -> &[u8] {
        &self.encoded_envelope
    }
}

/// Provider terminal facts needed by the completion gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelTerminalRecord {
    response_digest: Sha256Digest,
    normal_terminal: bool,
    incomplete_items: bool,
    usage_settled: bool,
}

impl ModelTerminalRecord {
    #[must_use]
    pub const fn new(
        response_digest: Sha256Digest,
        normal_terminal: bool,
        incomplete_items: bool,
        usage_settled: bool,
    ) -> Self {
        Self { response_digest, normal_terminal, incomplete_items, usage_settled }
    }
    #[must_use]
    pub const fn response_digest(self) -> Sha256Digest {
        self.response_digest
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
}

/// Provider retry mode selected after consulting C5 recovery facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRetryClass {
    /// Resume the same canonical provider stream at this exact cursor.
    ExactResume { cursor: u64 },
    /// Start a semantically safe new request rather than resume uncertain work.
    SafeNewRequest,
}

/// Failure and successor request facts for one bounded provider retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderRetryRecord {
    failure_digest: Sha256Digest,
    request_digest: Sha256Digest,
    class: ProviderRetryClass,
}

impl ProviderRetryRecord {
    #[must_use]
    pub const fn new(
        failure_digest: Sha256Digest,
        request_digest: Sha256Digest,
        class: ProviderRetryClass,
    ) -> Self {
        Self { failure_digest, request_digest, class }
    }
    #[must_use]
    pub const fn failure_digest(self) -> Sha256Digest {
        self.failure_digest
    }
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    #[must_use]
    pub const fn class(self) -> ProviderRetryClass {
        self.class
    }
}

/// Closed, exhaustive pure agent command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentCommandKind {
    ContextPrepared(ContextRecord),
    ModelRequestStarted { call_id: ModelCallId, request_digest: Sha256Digest },
    ProviderEventObserved(ProviderEventRecord),
    ProviderRetryScheduled(ProviderRetryRecord),
    ToolCallsProposed { terminal: ModelTerminalRecord, proposals: Vec<ToolProposal> },
    CompletionProposed { terminal: ModelTerminalRecord, proposal: CompletionProposal },
    AuthorizationStarted,
    ToolAuthorized { ordinal: ToolOrdinal, authority_digest: Sha256Digest },
    ToolDenied { ordinal: ToolOrdinal, result: ToolResultRecord },
    ToolExecutionStarted,
    ToolDispatched { ordinal: ToolOrdinal },
    ToolActivated { ordinal: ToolOrdinal },
    ToolProgressObserved { ordinal: ToolOrdinal, sequence: u32, progress_digest: Sha256Digest },
    ToolCompleted { ordinal: ToolOrdinal, result: ToolResultRecord },
    ResultRecordingStarted,
    ResultsRecorded { transcript_digest: Sha256Digest },
    Paused,
    Resumed { recovery_checked: bool },
    CancellationRequested,
    CancellationFinished,
    Failed(AgentFailure),
    Exhausted(AgentFailure),
    CompletionCommitted,
}

/// Causally fenced command. Rejections leave the supplied state unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentCommand {
    command_id: CommandId,
    event_id: EventId,
    expected_revision: RevisionNumber,
    expected_state_digest: Sha256Digest,
    kind: AgentCommandKind,
}

impl AgentCommand {
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        event_id: EventId,
        expected_revision: RevisionNumber,
        expected_state_digest: Sha256Digest,
        kind: AgentCommandKind,
    ) -> Self {
        Self { command_id, event_id, expected_revision, expected_state_digest, kind }
    }
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    #[must_use]
    pub const fn expected_revision(&self) -> RevisionNumber {
        self.expected_revision
    }
    #[must_use]
    pub const fn expected_state_digest(&self) -> Sha256Digest {
        self.expected_state_digest
    }
    #[must_use]
    pub const fn kind(&self) -> &AgentCommandKind {
        &self.kind
    }
}

#[allow(dead_code)]
const fn _active_phase_is_closed(phase: ActivePhase) -> u8 {
    phase as u8
}
