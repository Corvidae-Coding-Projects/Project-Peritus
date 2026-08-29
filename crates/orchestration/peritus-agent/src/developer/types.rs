//! Checked inputs, outputs, and effect ports for the production developer loop.

use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, MediaInput, Message, ToolDefinition,
};
use peritus_provider_core::CancellationToken;
use peritus_types::Sha256Digest;

use super::DeveloperLoopError;

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
}

/// Durable evidence for one deterministic transcript compaction.
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
    /// A checked deterministic compaction replaced complete prior tool exchanges.
    ContextCompaction(&'a DeveloperContextCompaction),
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
    /// Number of deterministic transcript compactions applied during the role.
    pub compactions: u16,
    /// Complete replay messages, useful to a same-role continuation.
    pub messages: Vec<Message>,
}
