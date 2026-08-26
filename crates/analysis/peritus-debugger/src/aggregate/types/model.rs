//! Frozen model budgets, retries, work state, and attempt history.

use crate::{DebuggerError, ModelAnalysisId};
use peritus_types::Sha256Digest;

/// Frozen model resource budget for one job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "max prefixes distinguish hard ceilings from observed model accounting"
)]
pub struct ModelBudget {
    max_events: u64,
    max_output_bytes: u64,
    max_input_tokens: u64,
    max_output_tokens: u64,
    max_total_tokens: u64,
}

impl ModelBudget {
    /// Constructs nonzero bounded model ceilings.
    ///
    /// # Errors
    ///
    /// Rejects zero ceilings or a total-token ceiling below either directional ceiling.
    pub fn new(
        max_events: u64,
        max_output_bytes: u64,
        max_input_tokens: u64,
        max_output_tokens: u64,
        max_total_tokens: u64,
    ) -> Result<Self, DebuggerError> {
        if [max_events, max_output_bytes, max_input_tokens, max_output_tokens, max_total_tokens]
            .contains(&0)
            || max_total_tokens < max_input_tokens.max(max_output_tokens)
        {
            return Err(super::invalid("model budget is zero or internally inconsistent"));
        }
        Ok(Self {
            max_events,
            max_output_bytes,
            max_input_tokens,
            max_output_tokens,
            max_total_tokens,
        })
    }
    /// Maximum normalized C5 events.
    #[must_use]
    pub const fn max_events(self) -> u64 {
        self.max_events
    }
    /// Maximum canonical structured output bytes.
    #[must_use]
    pub const fn max_output_bytes(self) -> u64 {
        self.max_output_bytes
    }
    /// Maximum observed input tokens.
    #[must_use]
    pub const fn max_input_tokens(self) -> u64 {
        self.max_input_tokens
    }
    /// Maximum observed output tokens.
    #[must_use]
    pub const fn max_output_tokens(self) -> u64 {
        self.max_output_tokens
    }
    /// Maximum observed total tokens.
    #[must_use]
    pub const fn max_total_tokens(self) -> u64 {
        self.max_total_tokens
    }
}
/// Frozen retry policy for optional model analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelRetryPolicy {
    max_attempts: u16,
    max_delay_ticks: u64,
}

impl ModelRetryPolicy {
    /// Constructs a bounded retry policy. One attempt means no retry.
    ///
    /// # Errors
    ///
    /// Rejects zero or excessive attempts and zero scheduling delay.
    pub fn new(max_attempts: u16, max_delay_ticks: u64) -> Result<Self, DebuggerError> {
        if max_attempts == 0 || max_attempts > 32 || max_delay_ticks == 0 {
            return Err(super::invalid("model retry policy exceeds compiled bounds"));
        }
        Ok(Self { max_attempts, max_delay_ticks })
    }
    /// Maximum total attempts.
    #[must_use]
    pub const fn max_attempts(self) -> u16 {
        self.max_attempts
    }
    /// Maximum scheduling delay in caller monotonic ticks.
    #[must_use]
    pub const fn max_delay_ticks(self) -> u64 {
        self.max_delay_ticks
    }
}

/// Redaction-safe closed model-attempt failure taxonomy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelAttemptFailureCode {
    /// Provider rejected the request before a stream was owned.
    ProviderStart,
    /// Provider stream transport failed.
    ProviderStream,
    /// Normalized event grammar was malformed.
    MalformedStream,
    /// Terminal outcome was not successful.
    UnsuccessfulTerminal,
    /// Output was absent, multiple, or not structured.
    InvalidOutputShape,
    /// Structured JSON failed the E2 schema or provenance checks.
    InvalidProposal,
    /// An E2 event, byte, or token budget was exhausted.
    BudgetExceeded,
    /// Cooperative cancellation won.
    Cancelled,
}

impl ModelAttemptFailureCode {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::ProviderStart => 1,
            Self::ProviderStream => 2,
            Self::MalformedStream => 3,
            Self::UnsuccessfulTerminal => 4,
            Self::InvalidOutputShape => 5,
            Self::InvalidProposal => 6,
            Self::BudgetExceeded => 7,
            Self::Cancelled => 8,
        }
    }
    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::ProviderStart),
            2 => Ok(Self::ProviderStream),
            3 => Ok(Self::MalformedStream),
            4 => Ok(Self::UnsuccessfulTerminal),
            5 => Ok(Self::InvalidOutputShape),
            6 => Ok(Self::InvalidProposal),
            7 => Ok(Self::BudgetExceeded),
            8 => Ok(Self::Cancelled),
            _ => Err(super::invalid("unknown model-attempt failure tag")),
        }
    }
}

/// Redaction-safe durable model-attempt failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelAttemptFailure {
    model_id: ModelAnalysisId,
    attempt: u16,
    code: ModelAttemptFailureCode,
    retryable: bool,
    diagnostic_digest: Sha256Digest,
    event_count: u64,
    total_tokens: u64,
}

impl ModelAttemptFailure {
    /// Creates one exact safe attempt observation.
    ///
    /// # Errors
    ///
    /// Rejects attempt zero or a cancellation incorrectly marked retryable.
    #[allow(clippy::too_many_arguments, reason = "attempt accounting fields remain explicit")]
    pub fn new(
        model_id: ModelAnalysisId,
        attempt: u16,
        code: ModelAttemptFailureCode,
        retryable: bool,
        diagnostic_digest: Sha256Digest,
        event_count: u64,
        total_tokens: u64,
    ) -> Result<Self, DebuggerError> {
        if attempt == 0 || code == ModelAttemptFailureCode::Cancelled && retryable {
            return Err(super::invalid("model failure attempt or retry classification is invalid"));
        }
        Ok(Self {
            model_id,
            attempt,
            code,
            retryable,
            diagnostic_digest,
            event_count,
            total_tokens,
        })
    }
    /// Analysis identity.
    #[must_use]
    pub const fn model_id(self) -> ModelAnalysisId {
        self.model_id
    }
    /// One-based attempt number.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
    /// Stable failure code.
    #[must_use]
    pub const fn code(self) -> ModelAttemptFailureCode {
        self.code
    }
    /// Whether exact policy permits a retry.
    #[must_use]
    pub const fn retryable(self) -> bool {
        self.retryable
    }
    /// Digest of safe normalized diagnostic metadata.
    #[must_use]
    pub const fn diagnostic_digest(self) -> Sha256Digest {
        self.diagnostic_digest
    }
    /// Normalized events observed.
    #[must_use]
    pub const fn event_count(self) -> u64 {
        self.event_count
    }
    /// Total tokens observed.
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
}

/// Durable result retained for every settled model attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelAttemptResult {
    /// A strict proposal passed complete E2 validation.
    Proposal {
        /// Canonical checked proposal digest.
        proposal_digest: Sha256Digest,
        /// Canonical structured-output digest.
        output_digest: Sha256Digest,
        /// Canonical structured-output byte count.
        output_bytes: u64,
        /// Normalized event count.
        event_count: u64,
        /// Input token high water.
        input_tokens: u64,
        /// Output token high water.
        output_tokens: u64,
        /// Total token high water.
        total_tokens: u64,
    },
    /// No model bytes were admitted to the report.
    Failure(ModelAttemptFailure),
}

/// One immutable settled model-attempt history entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelAttemptObservation {
    model_id: ModelAnalysisId,
    attempt: u16,
    result: ModelAttemptResult,
}

impl ModelAttemptObservation {
    /// Creates one attempt observation and checks nested identity consistency.
    ///
    /// # Errors
    ///
    /// Rejects attempt zero or a nested failure bound to another analysis or attempt.
    pub fn new(
        model_id: ModelAnalysisId,
        attempt: u16,
        result: ModelAttemptResult,
    ) -> Result<Self, DebuggerError> {
        if attempt == 0
            || matches!(result, ModelAttemptResult::Failure(failure) if failure.model_id() != model_id || failure.attempt() != attempt)
        {
            return Err(super::invalid("model attempt history identity is inconsistent"));
        }
        Ok(Self { model_id, attempt, result })
    }
    /// Analysis identity.
    #[must_use]
    pub const fn model_id(self) -> ModelAnalysisId {
        self.model_id
    }
    /// One-based attempt.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
    /// Exact settled result.
    #[must_use]
    pub const fn result(self) -> ModelAttemptResult {
        self.result
    }
}

/// Durable optional-model work state inside a debugger job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelWorkState {
    /// Initial or retry directive is durable and eligible to claim at the given tick.
    Pending {
        /// Exact one-based attempt.
        attempt: u16,
        /// Caller monotonic tick before which the attempt is ineligible.
        not_before_tick: u64,
    },
    /// The exact directive was claimed and attempt start committed.
    Running {
        /// Exact one-based attempt.
        attempt: u16,
        /// Positive caller monotonic tick at attempt start.
        started_at_tick: u64,
    },
    /// One retryable failure awaits a separate scheduling transition.
    AwaitingRetry {
        /// Exact completed attempt.
        attempt: u16,
        /// Durable failure that permits a bounded retry.
        failure: ModelAttemptFailure,
    },
    /// A proposal passed all E2 validation.
    Validated {
        /// Exact successful attempt.
        attempt: u16,
        /// Canonical checked proposal digest.
        proposal_digest: Sha256Digest,
    },
    /// Optional model work ended without contributing a proposal.
    Rejected {
        /// Exact terminal attempt.
        attempt: u16,
        /// Durable nonretryable failure.
        failure: ModelAttemptFailure,
    },
}

/// Complete durable optional-model accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelProgress {
    id: ModelAnalysisId,
    plan_digest: Sha256Digest,
    request_digest: Sha256Digest,
    budget: ModelBudget,
    retry_policy: ModelRetryPolicy,
    state: ModelWorkState,
}

impl ModelProgress {
    pub(crate) const fn new(
        id: ModelAnalysisId,
        plan_digest: Sha256Digest,
        request_digest: Sha256Digest,
        budget: ModelBudget,
        retry_policy: ModelRetryPolicy,
    ) -> Self {
        Self {
            id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
            state: ModelWorkState::Pending { attempt: 1, not_before_tick: 0 },
        }
    }
    /// Analysis identity.
    #[must_use]
    pub const fn id(self) -> ModelAnalysisId {
        self.id
    }
    /// Frozen plan digest.
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest {
        self.plan_digest
    }
    /// C5 semantic request digest.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Frozen budget.
    #[must_use]
    pub const fn budget(self) -> ModelBudget {
        self.budget
    }
    /// Frozen retry policy.
    #[must_use]
    pub const fn retry_policy(self) -> ModelRetryPolicy {
        self.retry_policy
    }
    /// Current work state.
    #[must_use]
    pub const fn state(self) -> ModelWorkState {
        self.state
    }
    pub(crate) const fn with_state(mut self, state: ModelWorkState) -> Self {
        self.state = state;
        self
    }
}
