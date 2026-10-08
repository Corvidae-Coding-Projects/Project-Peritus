//! Checked inputs, outputs, and effect ports for the production developer loop.

use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, MediaInput, Message, ModelRequest, OutcomeCertainty,
    ToolDefinition,
};
use peritus_provider_core::CancellationToken;
use peritus_types::{ProviderProfileId, Sha256Digest};

use super::{DeveloperLoopError, DeveloperUsage};

/// Explicit bounds for one inspect/edit/run/test loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeveloperLoopLimits {
    model_turns: u16,
    tool_calls: u32,
    max_output_tokens: u64,
    segment_continuation: bool,
    segment_sequence: u32,
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
            max_output_tokens: 32_768,
            segment_continuation: false,
            segment_sequence: 0,
        })
    }

    /// Treats these bounds as one physical scheduling segment of a durable logical invocation.
    ///
    /// A context port must durably checkpoint the exact continuation before the loop reports a
    /// segment boundary. Without that owner the same bounds remain an explicit exhaustion.
    #[must_use]
    pub const fn with_segment_continuation(mut self) -> Self {
        self.segment_continuation = true;
        self
    }

    /// Selects the physical segment identity within one durable logical invocation.
    ///
    /// Segment zero preserves canonical legacy request and receipt identities. Later segments use
    /// a suffix so their model requests and effects cannot collide with an earlier segment.
    #[must_use]
    pub const fn with_segment_sequence(mut self, segment_sequence: u32) -> Self {
        self.segment_sequence = segment_sequence;
        self
    }

    /// Compatibility constructor retained for callers compiled against the former attempt cap.
    ///
    /// Safe, definitely-unaccepted requests now reconnect until cancellation or a real elapsed
    /// horizon; this value is validated for its former nonzero precondition and otherwise ignored.
    ///
    /// # Errors
    /// Rejects zero, preserving the former constructor's minimum-value contract.
    pub const fn with_max_attempts_per_turn(
        self,
        max_attempts_per_turn: u8,
    ) -> Result<Self, DeveloperLoopError> {
        if max_attempts_per_turn == 0 {
            return Err(DeveloperLoopError::LimitExceeded);
        }
        Ok(self)
    }

    /// Selects a generation allowance for each provider turn in this loop.
    /// The immutable provider profile supplies the actual model ceiling at request construction.
    ///
    /// # Errors
    /// Rejects zero; this policy does not impose another host output-token ceiling.
    pub const fn with_max_output_tokens(
        mut self,
        max_output_tokens: u64,
    ) -> Result<Self, DeveloperLoopError> {
        if max_output_tokens == 0 {
            return Err(DeveloperLoopError::LimitExceeded);
        }
        self.max_output_tokens = max_output_tokens;
        Ok(self)
    }

    /// Uses the actual negotiated provider output allowance on every model turn.
    /// This also follows a provider selection changed at a later live-input boundary.
    #[must_use]
    pub const fn with_provider_output(mut self) -> Self {
        self.max_output_tokens = u64::MAX;
        self
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

    /// Maximum requested output tokens for each provider turn.
    #[must_use]
    pub const fn max_output_tokens(self) -> u64 {
        self.max_output_tokens
    }

    /// Whether a reached physical bound requests a durable scheduling continuation.
    #[must_use]
    pub const fn permits_segment_continuation(self) -> bool {
        self.segment_continuation
    }

    /// Physical segment identity; zero is the unchanged legacy request namespace.
    #[must_use]
    pub const fn segment_sequence(self) -> u32 {
        self.segment_sequence
    }

    /// Returns the exact provider/effect request namespace for this physical segment.
    #[must_use]
    pub fn segment_request_prefix(self, logical_prefix: &str) -> String {
        Self::request_prefix_for_segment(logical_prefix, self.segment_sequence)
    }

    /// Derives a physical segment namespace while preserving segment-zero compatibility.
    #[must_use]
    pub fn request_prefix_for_segment(logical_prefix: &str, segment_sequence: u32) -> String {
        if segment_sequence == 0 {
            logical_prefix.to_owned()
        } else {
            format!("{logical_prefix}-segment-{segment_sequence}")
        }
    }
}

/// Fully resolved inputs for one tool-capable developer role.
pub struct DeveloperLoopRequest {
    /// Durable native runtime namespace for this host task and role, independent of memory policy.
    /// Semantic compaction calls never inherit this namespace.
    pub local_session_directory: Option<std::path::PathBuf>,
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

/// Durable checked decision to wait before another definitely-unaccepted provider attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeveloperRetryRecord {
    turn: u16,
    attempt: u64,
    request_id_digest: Sha256Digest,
    request_fingerprint: Sha256Digest,
    provider_profile_id: ProviderProfileId,
    native_session_digest: Option<Sha256Digest>,
    provider_selection_digest: Option<Sha256Digest>,
    certainty: OutcomeCertainty,
    elapsed_millis: u64,
    delay_millis: u64,
    next_eligible_unix_millis: u64,
    retry_after_millis: Option<u64>,
    reason: DeveloperRetryReason,
}

impl DeveloperRetryRecord {
    pub(crate) const fn new(
        identity: (
            u16,
            u64,
            Sha256Digest,
            Sha256Digest,
            ProviderProfileId,
            Option<Sha256Digest>,
            Option<Sha256Digest>,
        ),
        timing: (u64, u64, u64),
        retry_after_millis: Option<u64>,
        reason: DeveloperRetryReason,
    ) -> Self {
        let (
            turn,
            attempt,
            request_id_digest,
            request_fingerprint,
            provider_profile_id,
            native_session_digest,
            provider_selection_digest,
        ) = identity;
        let (elapsed_millis, delay_millis, next_eligible_unix_millis) = timing;
        Self {
            turn,
            attempt,
            request_id_digest,
            request_fingerprint,
            provider_profile_id,
            native_session_digest,
            provider_selection_digest,
            certainty: OutcomeCertainty::DefinitelyNotAccepted,
            elapsed_millis,
            delay_millis,
            next_eligible_unix_millis,
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
    pub const fn attempt(self) -> u64 {
        self.attempt
    }

    /// Digest of the exact caller request identity retained across reconnects.
    #[must_use]
    pub const fn request_id_digest(self) -> Sha256Digest {
        self.request_id_digest
    }

    /// Digest of the immutable semantic request retained across reconnects.
    #[must_use]
    pub const fn request_fingerprint(self) -> Sha256Digest {
        self.request_fingerprint
    }

    /// Immutable provider profile selected for the retrying logical turn.
    #[must_use]
    pub const fn provider_profile_id(self) -> ProviderProfileId {
        self.provider_profile_id
    }

    /// Digest of the exact host-owned native session namespace, when present.
    #[must_use]
    pub const fn native_session_digest(self) -> Option<Sha256Digest> {
        self.native_session_digest
    }

    /// Digest of the exact persisted host model selection, when the host can prove it.
    #[must_use]
    pub const fn provider_selection_digest(self) -> Option<Sha256Digest> {
        self.provider_selection_digest
    }

    /// Acceptance certainty that made a fresh dispatch legal.
    #[must_use]
    pub const fn certainty(self) -> OutcomeCertainty {
        self.certainty
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

    /// Durable wall-clock time before which the retained request must not be dispatched.
    #[must_use]
    pub const fn next_eligible_unix_millis(self) -> u64 {
        self.next_eligible_unix_millis
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

/// Restart state recovered from a durably scheduled safe retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeveloperRetryRecovery {
    next_attempt: u64,
    next_eligible_unix_millis: u64,
}

impl DeveloperRetryRecovery {
    /// Creates checked restart state from a committed retry schedule.
    #[must_use]
    pub const fn new(next_attempt: u64, next_eligible_unix_millis: u64) -> Self {
        Self { next_attempt, next_eligible_unix_millis }
    }

    /// Exact next provider attempt identity.
    #[must_use]
    pub const fn next_attempt(self) -> u64 {
        self.next_attempt
    }

    /// Durable wall-clock eligibility for that attempt.
    #[must_use]
    pub const fn next_eligible_unix_millis(self) -> u64 {
        self.next_eligible_unix_millis
    }
}

/// Durable disposition of the last admitted provider request in one logical turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperRetryDisposition {
    /// An explicit provider terminal settled the request.
    Settled,
    /// Submission may have been accepted and must be reconciled instead of resent.
    ReconciliationRequired,
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
    /// Records observed work independently of success, before the next fallible boundary.
    /// Existing trace-only hosts may ignore accounting; product hosts enforce shared budgets here.
    ///
    /// # Errors
    /// Returns a host accounting or budget failure, stopping further work.
    fn account(
        &mut self,
        _event: super::DeveloperAccountingEvent,
    ) -> Result<(), DeveloperLoopError> {
        Ok(())
    }

    /// Recovers the latest retry boundary for this exact logical request and provider profile.
    ///
    /// A durable trace implementation rejects an admitted or ambiguous request that lacks a safe
    /// retry schedule. Trace-only hosts may use the default empty recovery state.
    ///
    /// # Errors
    /// Returns a persistence or reconciliation failure before another request is admitted.
    fn recover_retry(
        &mut self,
        _request_prefix: &str,
        _turn: u16,
        _provider_profile_id: ProviderProfileId,
        _native_session_digest: Option<Sha256Digest>,
        _request_fingerprint: Sha256Digest,
    ) -> Result<Option<DeveloperRetryRecovery>, DeveloperLoopError> {
        Ok(None)
    }

    /// Commits admission of one exact provider request before provider execution can begin.
    ///
    /// # Errors
    /// Returns a persistence failure; the provider request must not be sent.
    fn begin_retry_attempt(
        &mut self,
        _turn: u16,
        _attempt: u64,
        _request: &ModelRequest,
    ) -> Result<(), DeveloperLoopError> {
        Ok(())
    }

    /// Commits the terminal or ambiguous disposition of an admitted provider request.
    ///
    /// # Errors
    /// Returns a persistence failure before control leaves the request boundary.
    fn finish_retry_attempt(
        &mut self,
        _turn: u16,
        _attempt: u64,
        _request: &ModelRequest,
        _disposition: DeveloperRetryDisposition,
    ) -> Result<(), DeveloperLoopError> {
        Ok(())
    }

    /// Retires one exact definitely-unaccepted schedule after newer governing input supersedes it.
    ///
    /// This transition is valid only while the trace's latest retry state is the matching safe
    /// schedule. An admitted or ambiguous request must remain available for reconciliation.
    ///
    /// # Errors
    /// Returns a persistence or state-transition failure before stale control is released.
    fn supersede_retry(
        &mut self,
        _request_prefix: &str,
        _turn: u16,
        _scheduled_attempt: u64,
    ) -> Result<(), DeveloperLoopError> {
        Err(DeveloperLoopError::Trace(
            "retry trace does not support durable schedule supersession".to_owned(),
        ))
    }

    /// Retires a matching safe schedule only when its recorded explicit provider selection differs.
    /// Missing provenance or an unchanged selection cannot authorize retirement. Admitted and
    /// ambiguous requests remain reconciliation barriers.
    ///
    /// # Errors
    /// Returns a persistence or reconciliation failure before a new profile may be dispatched.
    fn supersede_retry_for_provider_selection(
        &mut self,
        _request_prefix: &str,
        _turn: u16,
        _current_selection: Sha256Digest,
    ) -> Result<bool, DeveloperLoopError> {
        Ok(false)
    }

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
    /// Number of safe provider retries completed during the role.
    pub retries: u64,
    /// Aggregate normalized usage across every completed provider response.
    pub usage: DeveloperUsage,
    /// Complete replay messages, useful to a same-role continuation.
    pub messages: Vec<Message>,
}
