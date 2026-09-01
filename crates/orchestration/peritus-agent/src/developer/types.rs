//! Checked inputs, outputs, and effect ports for the production developer loop.

use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, MediaInput, Message, ToolDefinition,
};
use peritus_provider_core::CancellationToken;
use peritus_types::Sha256Digest;

use super::{DeveloperLoopError, DeveloperUsage};

/// Explicit bounds for one inspect/edit/run/test loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeveloperLoopLimits {
    model_turns: u16,
    tool_calls: u32,
    attempts_per_turn: u8,
    max_output_tokens: u64,
}

impl DeveloperLoopLimits {
    /// Creates nonzero production loop bounds.
    ///
    /// # Errors
    /// Rejects zero or unreasonably wide loops.
    pub const fn new(
        max_model_turns: u16,
        max_tool_calls: u32,
    ) -> Result<Self, DeveloperLoopError> {
        if max_model_turns == 0
            || max_model_turns > 128
            || max_tool_calls == 0
            || max_tool_calls > 2_048
        {
            return Err(DeveloperLoopError::LimitExceeded);
        }
        Ok(Self {
            model_turns: max_model_turns,
            tool_calls: max_tool_calls,
            attempts_per_turn: 3,
            max_output_tokens: 32_768,
        })
    }

    /// Overrides the attempts available to recover one logical model turn.
    ///
    /// # Errors
    /// Rejects zero or an attempt count wide enough to hide a persistent provider failure.
    pub const fn with_max_attempts_per_turn(
        mut self,
        max_attempts_per_turn: u8,
    ) -> Result<Self, DeveloperLoopError> {
        if max_attempts_per_turn == 0 || max_attempts_per_turn > 8 {
            return Err(DeveloperLoopError::LimitExceeded);
        }
        self.attempts_per_turn = max_attempts_per_turn;
        Ok(self)
    }

    /// Applies a smaller generation ceiling to each provider turn in this loop.
    ///
    /// # Errors
    /// Rejects zero or a value wider than the production developer-loop ceiling.
    pub const fn with_max_output_tokens(
        mut self,
        max_output_tokens: u64,
    ) -> Result<Self, DeveloperLoopError> {
        if max_output_tokens == 0 || max_output_tokens > 32_768 {
            return Err(DeveloperLoopError::LimitExceeded);
        }
        self.max_output_tokens = max_output_tokens;
        Ok(self)
    }

    /// Maximum provider turns.
    #[must_use]
    pub const fn max_model_turns(self) -> u16 {
        self.model_turns
    }

    /// Maximum completed tool calls.
    #[must_use]
    pub const fn max_tool_calls(self) -> u32 {
        self.tool_calls
    }

    /// Maximum fresh attempts for one logical provider turn.
    #[must_use]
    pub const fn max_attempts_per_turn(self) -> u8 {
        self.attempts_per_turn
    }

    /// Maximum requested output tokens for each provider turn.
    #[must_use]
    pub const fn max_output_tokens(self) -> u64 {
        self.max_output_tokens
    }
}

/// Fully resolved inputs for one tool-capable developer role.
pub struct DeveloperLoopRequest {
    /// Stable provider request prefix.
    pub request_prefix: String,
    /// Developer role policy.
    pub system: String,
    /// Current task, conversation, and prior findings.
    pub prompt: String,
    /// Bounded media attached to the initial user turn in prompt-described order.
    pub attachments: Vec<MediaInput>,
    /// Provider-visible application tools.
    pub tools: Vec<ToolDefinition>,
    /// Explicit loop bounds.
    pub limits: DeveloperLoopLimits,
    /// Shared provider cancellation.
    pub cancellation: CancellationToken,
}

/// Bounded application observation returned to the model.
pub struct DeveloperToolObservation {
    /// Canonical structured result.
    pub output: CanonicalJson,
    /// Whether execution failed while still producing an actionable observation.
    pub is_error: bool,
}

/// Executes already parsed provider tool calls against one explicitly supplied workspace.
pub trait DeveloperToolExecutor: Send {
    /// Executes one call and returns a model-safe observation.
    ///
    /// # Errors
    /// Returns a structural dispatch failure. Ordinary command failures should be represented as
    /// `DeveloperToolObservation { is_error: true, .. }` so the model can inspect and retry.
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError>;

    /// Explains why a text-only model response cannot yet complete this tool session.
    ///
    /// The developer loop feeds this reason back into the same conversation and keeps the
    /// executor alive, preserving partial inspection evidence across the correction. Executors
    /// without a completion precondition use the default ready state.
    fn completion_blocker(&self) -> Option<String> {
        None
    }

    /// Returns and clears a deterministic correction after an unproductive tool sequence.
    ///
    /// The developer loop appends this as a user message after the current tool batch. Executors
    /// without application-specific progress evidence use the default no-feedback behavior.
    fn take_progress_feedback(&mut self) -> Option<String> {
        None
    }
}

/// Durable evidence for one semantic or deterministic transcript compaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeveloperContextCompaction {
    policy_digest: Sha256Digest,
    source_digest: Sha256Digest,
    replacement_digest: Sha256Digest,
    source_messages: u16,
    replaced_tokens: u64,
    replacement_tokens: u64,
}

impl DeveloperContextCompaction {
    pub(crate) const fn new(
        digests: [Sha256Digest; 3],
        source_messages: u16,
        replaced_tokens: u64,
        replacement_tokens: u64,
    ) -> Self {
        let [policy_digest, source_digest, replacement_digest] = digests;
        Self {
            policy_digest,
            source_digest,
            replacement_digest,
            source_messages,
            replaced_tokens,
            replacement_tokens,
        }
    }

    /// Exact revision of the deterministic compaction policy.
    #[must_use]
    pub const fn policy_digest(self) -> Sha256Digest {
        self.policy_digest
    }

    /// Digest of every exact source message replaced by this record.
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }

    /// Digest of the installed model-visible replacement.
    #[must_use]
    pub const fn replacement_digest(self) -> Sha256Digest {
        self.replacement_digest
    }

    /// Number of complete source messages replaced atomically.
    #[must_use]
    pub const fn source_messages(self) -> u16 {
        self.source_messages
    }

    /// Conservative source token estimate removed from the active transcript.
    #[must_use]
    pub const fn replaced_tokens(self) -> u64 {
        self.replaced_tokens
    }

    /// Conservative replacement token estimate installed in the transcript.
    #[must_use]
    pub const fn replacement_tokens(self) -> u64 {
        self.replacement_tokens
    }
}

/// Stable reason for scheduling another bounded provider attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperRetryReason {
    /// The provider completed successfully but returned no usable text or tool call.
    EmptyResponse,
    /// A normalized provider failure explicitly permits a fresh request.
    RetryableProviderResponse,
    /// Connection establishment failed before request submission.
    Connection,
    /// The provider transport was interrupted.
    Transport,
    /// The provider stream was malformed or incomplete.
    MalformedStream,
}

impl DeveloperRetryReason {
    /// Stable machine-readable reason stored in product traces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyResponse => "empty_response",
            Self::RetryableProviderResponse => "retryable_provider_response",
            Self::Connection => "connection",
            Self::Transport => "transport",
            Self::MalformedStream => "malformed_stream",
        }
    }
}

/// Durable checked decision to wait before another provider attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeveloperRetryRecord {
    turn: u16,
    attempt: u8,
    max_attempts: u8,
    elapsed_millis: u64,
    delay_millis: u64,
    retry_after_millis: Option<u64>,
    reason: DeveloperRetryReason,
}

impl DeveloperRetryRecord {
    pub(crate) const fn new(
        position: (u16, u8, u8, u64, u64),
        retry_after_millis: Option<u64>,
        reason: DeveloperRetryReason,
    ) -> Self {
        let (turn, attempt, max_attempts, elapsed_millis, delay_millis) = position;
        Self {
            turn,
            attempt,
            max_attempts,
            elapsed_millis,
            delay_millis,
            retry_after_millis,
            reason,
        }
    }

    /// Logical model turn whose attempt will be retried.
    #[must_use]
    pub const fn turn(self) -> u16 {
        self.turn
    }

    /// Completed attempt after which the wait was selected.
    #[must_use]
    pub const fn attempt(self) -> u8 {
        self.attempt
    }

    /// Maximum attempts available to the logical turn.
    #[must_use]
    pub const fn max_attempts(self) -> u8 {
        self.max_attempts
    }

    /// Elapsed time when retry planning ran.
    #[must_use]
    pub const fn elapsed_millis(self) -> u64 {
        self.elapsed_millis
    }

    /// Checked bounded wait selected by retry policy.
    #[must_use]
    pub const fn delay_millis(self) -> u64 {
        self.delay_millis
    }

    /// Provider-supplied minimum wait, when available.
    #[must_use]
    pub const fn retry_after_millis(self) -> Option<u64> {
        self.retry_after_millis
    }

    /// Stable reason for the new attempt.
    #[must_use]
    pub const fn reason(self) -> DeveloperRetryReason {
        self.reason
    }
}

/// Exact trace event committed before D0 advances past an external observation.
pub enum DeveloperTraceEvent<'a> {
    /// Canonical normalized provider envelope.
    ProviderEnvelope(&'a [u8]),
    /// Completed tool call and its application observation.
    ToolObservation {
        /// Provider call.
        call: &'a CompletedToolCall,
        /// Canonical application result.
        observation: &'a DeveloperToolObservation,
    },
    /// A checked semantic or deterministic compaction replaced complete prior history.
    ContextCompaction(&'a DeveloperContextCompaction),
    /// Checked retry policy scheduled another provider attempt after a bounded wait.
    RetryScheduled(&'a DeveloperRetryRecord),
}

/// Durable trace boundary owned by the production host.
pub trait DeveloperTrace: Send {
    /// Commits one exact event before the loop advances.
    ///
    /// # Errors
    /// Returns a redaction-safe persistence failure.
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError>;
}

/// Successful terminal response from a developer role.
pub struct DeveloperLoopOutcome {
    /// Final provider text for product-level parsing.
    pub text: String,
    /// Number of provider turns executed.
    pub model_turns: u16,
    /// Number of application tool calls observed.
    pub tool_calls: u32,
    /// Number of transcript compactions applied during the role.
    pub compactions: u16,
    /// Number of bounded provider retries completed during the role.
    pub retries: u16,
    /// Aggregate normalized usage across every completed provider response.
    pub usage: DeveloperUsage,
    /// Complete replay messages, useful to a same-role continuation.
    pub messages: Vec<Message>,
}
