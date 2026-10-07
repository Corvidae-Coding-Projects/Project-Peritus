//! Frozen model budgets, retries, work state, and attempt history.

use crate::{DebuggerError, ModelAnalysisId};
use peritus_policy::AuthorityInstant;
use peritus_model_protocol::Continuation;
use peritus_types::{ProviderProfileId, Sha256Digest};

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
/// Caller-owned retry stopping and millisecond-delay policy for optional model analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelRetryPolicy {
    stop_after_attempts: Option<u16>,
    max_delay_millis: u64,
}

impl ModelRetryPolicy {
    /// Constructs an explicitly bounded retry policy. One attempt means no retry.
    ///
    /// # Errors
    ///
    /// Rejects zero attempts or zero scheduling delay.
    pub fn new(max_attempts: u16, max_delay_millis: u64) -> Result<Self, DebuggerError> {
        if max_attempts == 0 || max_delay_millis == 0 {
            return Err(super::invalid("model retry policy has a zero bound"));
        }
        Ok(Self { stop_after_attempts: Some(max_attempts), max_delay_millis })
    }
    /// Constructs a persistent retry policy with no synthetic attempt exhaustion.
    ///
    /// The caller still controls every retry transition and may stop scheduling at any time.
    /// The `u16` attempt identity is a representation bound, not a work allowance.
    ///
    /// # Errors
    ///
    /// Rejects a zero scheduling delay.
    pub fn persistent(max_delay_millis: u64) -> Result<Self, DebuggerError> {
        if max_delay_millis == 0 {
            return Err(super::invalid("model retry policy has a zero delay bound"));
        }
        Ok(Self { stop_after_attempts: None, max_delay_millis })
    }
    pub(crate) fn from_encoded(
        max_attempts: u16,
        max_delay_ticks: u64,
    ) -> Result<Self, DebuggerError> {
        if max_attempts == 0 {
            Self::persistent(max_delay_ticks)
        } else {
            Self::new(max_attempts, max_delay_ticks)
        }
    }
    /// Explicit caller stopping point, or `None` for persistent retries.
    #[must_use]
    pub const fn max_attempts(self) -> Option<u16> {
        self.stop_after_attempts
    }
    /// Returns whether the caller policy permits this one-based attempt.
    #[must_use]
    pub const fn permits(self, attempt: u16) -> bool {
        attempt != 0
            && match self.stop_after_attempts {
                Some(maximum) => attempt <= maximum,
                None => true,
            }
    }
    /// Maximum scheduling delay in milliseconds.
    #[must_use]
    pub const fn max_delay_millis(self) -> u64 {
        self.max_delay_millis
    }
    /// Legacy name for the millisecond delay bound retained for source compatibility.
    #[must_use]
    pub const fn max_delay_ticks(self) -> u64 {
        self.max_delay_millis
    }
}

/// Durable proof of one caller-selected retry delay on the authority millisecond clock.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ModelRetrySchedule {
    scheduled_at: AuthorityInstant,
    delay_millis: u64,
    not_before: AuthorityInstant,
}

impl ModelRetrySchedule {
    /// Binds a nonzero delay to one exact authority-clock epoch and observation.
    ///
    /// # Errors
    ///
    /// Rejects zero delay or an unrepresentable same-epoch deadline.
    pub fn new(
        scheduled_at: AuthorityInstant,
        delay_millis: u64,
    ) -> Result<Self, DebuggerError> {
        if delay_millis == 0 {
            return Err(super::invalid("model retry delay is zero"));
        }
        let Some(tick_millis) = scheduled_at.tick_millis().checked_add(delay_millis) else {
            return Err(super::invalid("model retry deadline overflowed its clock epoch"));
        };
        Ok(Self {
            scheduled_at,
            delay_millis,
            not_before: AuthorityInstant::new(scheduled_at.epoch(), tick_millis),
        })
    }
    /// Exact authority-clock observation at scheduling.
    #[must_use]
    pub const fn scheduled_at(self) -> AuthorityInstant {
        self.scheduled_at
    }
    /// Proven bounded relative delay in milliseconds.
    #[must_use]
    pub const fn delay_millis(self) -> u64 {
        self.delay_millis
    }
    /// Same-epoch inclusive eligibility threshold.
    #[must_use]
    pub const fn not_before(self) -> AuthorityInstant {
        self.not_before
    }
    /// Classifies a current authority observation against the durable schedule.
    #[must_use]
    pub const fn admission(self, now: AuthorityInstant) -> Option<ModelStartBasis> {
        if now.epoch().get() == self.scheduled_at.epoch().get() {
            if now.tick_millis() >= self.not_before.tick_millis() {
                Some(ModelStartBasis::Scheduled)
            } else {
                None
            }
        } else if now.epoch().get() > self.scheduled_at.epoch().get()
            && now.tick_millis() >= self.delay_millis
        {
            Some(ModelStartBasis::RestartRebased)
        } else {
            None
        }
    }
}

/// Durable reason an authority-clock observation admitted an attempt start.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelStartBasis {
    /// The first attempt had no retry delay.
    Immediate,
    /// An accepted legacy bare-tick record admitted the start.
    LegacyTick,
    /// The original authority epoch reached its same-epoch deadline.
    Scheduled,
    /// A later authority epoch observed the complete delay after restart.
    RestartRebased,
}

impl ModelStartBasis {
    pub(crate) const fn tag(self) -> u8 {
        self as u8 + 1
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::Immediate),
            2 => Ok(Self::LegacyTick),
            3 => Ok(Self::Scheduled),
            4 => Ok(Self::RestartRebased),
            _ => Err(super::invalid("unknown model start-basis tag")),
        }
    }
}

/// Stage that produced a rich model-attempt failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelFailureOrigin {
    /// The configured profile did not validate against the frozen request.
    ProfileValidation,
    /// The provider failed while starting the request.
    ProviderStart,
    /// The owned stream failed after start returned.
    ProviderStream,
    /// The provider emitted a typed terminal failure.
    ProviderTerminal,
    /// C5 rejected normalized stream grammar.
    ModelProtocol,
    /// E2 rejected completed output shape or proposal semantics.
    OutputValidation,
    /// A cumulative job work allowance was reached.
    Budget,
    /// Cooperative or provider cancellation won.
    Cancellation,
}

impl ModelFailureOrigin {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::ProfileValidation => 1,
            Self::ProviderStart => 2,
            Self::ProviderStream => 3,
            Self::ProviderTerminal => 4,
            Self::ModelProtocol => 5,
            Self::OutputValidation => 6,
            Self::Budget => 7,
            Self::Cancellation => 8,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::ProfileValidation),
            2 => Ok(Self::ProviderStart),
            3 => Ok(Self::ProviderStream),
            4 => Ok(Self::ProviderTerminal),
            5 => Ok(Self::ModelProtocol),
            6 => Ok(Self::OutputValidation),
            7 => Ok(Self::Budget),
            8 => Ok(Self::Cancellation),
            _ => Err(super::invalid("unknown model-failure origin tag")),
        }
    }
}

/// Exact provider/core cause retained independently of the failure stage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelProviderFailureCause {
    /// Provider endpoint configuration was invalid.
    InvalidEndpoint,
    /// Credential configuration or resolution was invalid.
    InvalidCredential,
    /// Request did not satisfy the provider contract.
    InvalidRequest,
    /// HTTP metadata or operation was invalid.
    InvalidHttp,
    /// A provider/core resource limit was exceeded.
    LimitExceeded,
    /// Cancellation interrupted provider work.
    Cancelled,
    /// Connection establishment failed before submission.
    Connect,
    /// Transport failed after connection establishment.
    Transport,
    /// Provider framing or stream grammar was malformed.
    MalformedStream,
    /// Provider retry inputs were invalid.
    InvalidRetry,
    /// Provider transport configuration was invalid.
    Configuration,
    /// The route lacked a required capability.
    UnsupportedCapability,
    /// The provider or credential was unavailable.
    Unavailable,
    /// Provider authentication failed.
    Authentication,
    /// Credential lacked permission for the request.
    Permission,
    /// Requested model or resource was absent.
    NotFound,
    /// Provider asked the caller to wait before retrying.
    RateLimited,
    /// Account or project quota was exhausted.
    QuotaExhausted,
    /// Provider reported transient capacity or availability failure.
    TransientProvider,
    /// Submission may have been accepted without terminal truth.
    AmbiguousAcceptance,
    /// Provider payload was malformed.
    MalformedPayload,
    /// Stream ended without its required terminal.
    IncompleteStream,
    /// Provider work exceeded its deadline.
    Timeout,
    /// Provider explicitly refused the request.
    Refusal,
    /// Provider safety policy prevented output.
    Safety,
    /// Provider cause was retained without guessing a narrower class.
    Provider,
}

impl ModelProviderFailureCause {
    pub(crate) const fn tag(self) -> u8 {
        self as u8 + 1
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::InvalidEndpoint),
            2 => Ok(Self::InvalidCredential),
            3 => Ok(Self::InvalidRequest),
            4 => Ok(Self::InvalidHttp),
            5 => Ok(Self::LimitExceeded),
            6 => Ok(Self::Cancelled),
            7 => Ok(Self::Connect),
            8 => Ok(Self::Transport),
            9 => Ok(Self::MalformedStream),
            10 => Ok(Self::InvalidRetry),
            11 => Ok(Self::Configuration),
            12 => Ok(Self::UnsupportedCapability),
            13 => Ok(Self::Unavailable),
            14 => Ok(Self::Authentication),
            15 => Ok(Self::Permission),
            16 => Ok(Self::NotFound),
            17 => Ok(Self::RateLimited),
            18 => Ok(Self::QuotaExhausted),
            19 => Ok(Self::TransientProvider),
            20 => Ok(Self::AmbiguousAcceptance),
            21 => Ok(Self::MalformedPayload),
            22 => Ok(Self::IncompleteStream),
            23 => Ok(Self::Timeout),
            24 => Ok(Self::Refusal),
            25 => Ok(Self::Safety),
            26 => Ok(Self::Provider),
            _ => Err(super::invalid("unknown provider-failure cause tag")),
        }
    }
}

/// Furthest provider transport phase retained by E2.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ModelFailurePhase {
    /// No request bytes were submitted.
    BeforeSend,
    /// Connection establishment was in progress.
    Connecting,
    /// Request headers were being submitted.
    SendingHeaders,
    /// Request body may have been submitted.
    SendingBody,
    /// Request was sent and response headers were pending.
    AwaitingHeaders,
    /// Response headers were accepted and body reading had begun.
    ReadingBody,
    /// At least one normalized application event was observed.
    StreamObserved,
    /// Provider emitted a terminal outcome.
    Completed,
}

impl ModelFailurePhase {
    pub(crate) const fn tag(self) -> u8 {
        self as u8 + 1
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::BeforeSend),
            2 => Ok(Self::Connecting),
            3 => Ok(Self::SendingHeaders),
            4 => Ok(Self::SendingBody),
            5 => Ok(Self::AwaitingHeaders),
            6 => Ok(Self::ReadingBody),
            7 => Ok(Self::StreamObserved),
            8 => Ok(Self::Completed),
            _ => Err(super::invalid("unknown model-failure phase tag")),
        }
    }
}

/// Certainty that a failed provider request was accepted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelAcceptanceCertainty {
    /// Provider is known not to have accepted the request.
    DefinitelyNotAccepted,
    /// Provider may have accepted the request.
    MaybeAccepted,
    /// Provider accepted the request and emitted partial output.
    AcceptedPartial,
    /// Provider emitted an explicit terminal outcome.
    Terminal,
}

impl ModelAcceptanceCertainty {
    pub(crate) const fn tag(self) -> u8 {
        self as u8 + 1
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::DefinitelyNotAccepted),
            2 => Ok(Self::MaybeAccepted),
            3 => Ok(Self::AcceptedPartial),
            4 => Ok(Self::Terminal),
            _ => Err(super::invalid("unknown model acceptance-certainty tag")),
        }
    }
}

/// Exact next action that can change a failed provider outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelFailureRecovery {
    /// No retry can safely or usefully change the outcome.
    Stop,
    /// The same route may receive a safe new request.
    RetrySameRoute,
    /// Only the exact retained provider cursor may resume work.
    ResumeExact,
    /// Retry must wait for credential repair and a real readiness check.
    AwaitCredentialRepair,
    /// A separately authorized capable route may be selected.
    TryAuthorizedFallback,
    /// The context plan must be compacted before another request.
    CompactThenRetry,
    /// Retry requires an explicit caller decision.
    CallerDecision,
}

impl ModelFailureRecovery {
    pub(crate) const fn tag(self) -> u8 {
        self as u8 + 1
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::Stop),
            2 => Ok(Self::RetrySameRoute),
            3 => Ok(Self::ResumeExact),
            4 => Ok(Self::AwaitCredentialRepair),
            5 => Ok(Self::TryAuthorizedFallback),
            6 => Ok(Self::CompactThenRetry),
            7 => Ok(Self::CallerDecision),
            _ => Err(super::invalid("unknown model-failure recovery tag")),
        }
    }

    /// Whether the frozen route/request can be scheduled again after caller reconciliation.
    #[must_use]
    pub const fn permits_same_plan_retry(self) -> bool {
        matches!(
            self,
            Self::RetrySameRoute
                | Self::ResumeExact
                | Self::AwaitCredentialRepair
                | Self::TryAuthorizedFallback
                | Self::CallerDecision
        )
    }
}

/// Durable provider identity, recovery truth, and native continuation for one failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelFailureContext {
    profile_id: ProviderProfileId,
    profile_revision: u64,
    origin: ModelFailureOrigin,
    cause: ModelProviderFailureCause,
    recovery: ModelFailureRecovery,
    phase: ModelFailurePhase,
    acceptance: ModelAcceptanceCertainty,
    continuation: Option<Continuation>,
    http_status: Option<u16>,
    retry_after_millis: Option<u64>,
    observations_digest: Sha256Digest,
}

impl ModelFailureContext {
    #[allow(clippy::too_many_arguments, reason = "provider recovery evidence remains explicit")]
    pub(crate) fn new(
        profile_id: ProviderProfileId,
        profile_revision: u64,
        origin: ModelFailureOrigin,
        cause: ModelProviderFailureCause,
        recovery: ModelFailureRecovery,
        phase: ModelFailurePhase,
        acceptance: ModelAcceptanceCertainty,
        continuation: Option<Continuation>,
        http_status: Option<u16>,
        retry_after_millis: Option<u64>,
        observations_digest: Sha256Digest,
    ) -> Result<Self, DebuggerError> {
        if profile_revision == 0
            || recovery == ModelFailureRecovery::ResumeExact
                && !continuation.as_ref().is_some_and(|value| {
                    value.event_id().is_some() && value.sequence().is_some()
                })
        {
            return Err(super::invalid(
                "model failure profile revision or exact continuation is invalid",
            ));
        }
        Ok(Self {
            profile_id,
            profile_revision,
            origin,
            cause,
            recovery,
            phase,
            acceptance,
            continuation,
            http_status,
            retry_after_millis,
            observations_digest,
        })
    }
    /// Exact immutable provider-profile identity.
    #[must_use]
    pub const fn profile_id(&self) -> ProviderProfileId {
        self.profile_id
    }
    /// Exact immutable provider-profile revision.
    #[must_use]
    pub const fn profile_revision(&self) -> u64 {
        self.profile_revision
    }
    /// Boundary stage that observed the failure.
    #[must_use]
    pub const fn origin(&self) -> ModelFailureOrigin {
        self.origin
    }
    /// Typed provider/core failure cause.
    #[must_use]
    pub const fn cause(&self) -> ModelProviderFailureCause {
        self.cause
    }
    /// Recovery action that can change the outcome.
    #[must_use]
    pub const fn recovery(&self) -> ModelFailureRecovery {
        self.recovery
    }
    /// Furthest observed transport phase.
    #[must_use]
    pub const fn phase(&self) -> ModelFailurePhase {
        self.phase
    }
    /// Certainty of provider-side request acceptance.
    #[must_use]
    pub const fn acceptance(&self) -> ModelAcceptanceCertainty {
        self.acceptance
    }
    /// Native response/thread continuation retained for safe recovery.
    #[must_use]
    pub const fn continuation(&self) -> Option<&Continuation> {
        self.continuation.as_ref()
    }
    /// Provider HTTP status when one was observed.
    #[must_use]
    pub const fn http_status(&self) -> Option<u16> {
        self.http_status
    }
    /// Provider retry-after duration when one was observed.
    #[must_use]
    pub const fn retry_after_millis(&self) -> Option<u64> {
        self.retry_after_millis
    }
    /// Rolling digest binding ordered provider observations.
    #[must_use]
    pub const fn observations_digest(&self) -> Sha256Digest {
        self.observations_digest
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
    /// Rich provider/core cause is retained in the attached failure context.
    ProviderFailure,
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
            Self::ProviderFailure => 9,
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
            9 => Ok(Self::ProviderFailure),
            _ => Err(super::invalid("unknown model-attempt failure tag")),
        }
    }
}

/// Redaction-safe durable model-attempt failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelAttemptFailure {
    model_id: ModelAnalysisId,
    attempt: u16,
    code: ModelAttemptFailureCode,
    legacy_retryable: Option<bool>,
    context: Option<ModelFailureContext>,
    diagnostic_digest: Sha256Digest,
    event_count: u64,
    output_bytes: u64,
    input_tokens: u64,
    output_tokens: u64,
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
            legacy_retryable: Some(retryable),
            context: None,
            diagnostic_digest,
            event_count,
            output_bytes: 0,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens,
        })
    }
    /// Creates a rich failure that preserves provider identity, recovery truth, and accounting.
    #[allow(clippy::too_many_arguments, reason = "failure settlement fields remain explicit")]
    pub fn observed(
        model_id: ModelAnalysisId,
        attempt: u16,
        code: ModelAttemptFailureCode,
        context: ModelFailureContext,
        diagnostic_digest: Sha256Digest,
        event_count: u64,
        output_bytes: u64,
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: u64,
    ) -> Result<Self, DebuggerError> {
        if attempt == 0
            || code == ModelAttemptFailureCode::Cancelled
                && context.recovery() != ModelFailureRecovery::Stop
        {
            return Err(super::invalid("model failure attempt or recovery is invalid"));
        }
        Ok(Self {
            model_id,
            attempt,
            code,
            legacy_retryable: None,
            context: Some(context),
            diagnostic_digest,
            event_count,
            output_bytes,
            input_tokens,
            output_tokens,
            total_tokens,
        })
    }
    /// Analysis identity.
    #[must_use]
    pub const fn model_id(&self) -> ModelAnalysisId {
        self.model_id
    }
    /// One-based attempt number.
    #[must_use]
    pub const fn attempt(&self) -> u16 {
        self.attempt
    }
    /// Stable failure code.
    #[must_use]
    pub const fn code(&self) -> ModelAttemptFailureCode {
        self.code
    }
    /// Whether exact policy permits a retry.
    #[must_use]
    pub fn retryable(&self) -> bool {
        self.legacy_retryable.unwrap_or_else(|| {
            self.context
                .as_ref()
                .is_some_and(|context| context.recovery().permits_same_plan_retry())
        })
    }
    /// Rich provider failure context, absent only on accepted schema-v1 history.
    #[must_use]
    pub const fn context(&self) -> Option<&ModelFailureContext> {
        self.context.as_ref()
    }
    pub(crate) const fn is_legacy(&self) -> bool {
        self.context.is_none()
    }
    /// Digest of safe normalized diagnostic metadata.
    #[must_use]
    pub const fn diagnostic_digest(&self) -> Sha256Digest {
        self.diagnostic_digest
    }
    /// Normalized events observed.
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    /// Output bytes observed before failure settlement.
    #[must_use]
    pub const fn output_bytes(&self) -> u64 {
        self.output_bytes
    }
    /// Input-token high water observed before failure settlement.
    #[must_use]
    pub const fn input_tokens(&self) -> u64 {
        self.input_tokens
    }
    /// Output-token high water observed before failure settlement.
    #[must_use]
    pub const fn output_tokens(&self) -> u64 {
        self.output_tokens
    }
    /// Total tokens observed.
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.total_tokens
    }
}

/// Durable result retained for every settled model attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
            || matches!(&result, ModelAttemptResult::Failure(failure) if failure.model_id() != model_id || failure.attempt() != attempt)
        {
            return Err(super::invalid("model attempt history identity is inconsistent"));
        }
        Ok(Self { model_id, attempt, result })
    }
    /// Analysis identity.
    #[must_use]
    pub const fn model_id(&self) -> ModelAnalysisId {
        self.model_id
    }
    /// One-based attempt.
    #[must_use]
    pub const fn attempt(&self) -> u16 {
        self.attempt
    }
    /// Exact settled result.
    #[must_use]
    pub const fn result(&self) -> &ModelAttemptResult {
        &self.result
    }
}

/// Durable optional-model work state inside a debugger job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelWorkState {
    /// Initial or retry directive is durable and eligible to claim at the given tick.
    Pending {
        /// Exact one-based attempt.
        attempt: u16,
        /// Caller monotonic tick before which the attempt is ineligible.
        not_before_tick: u64,
    },
    /// A retry is bound to an epoch-scoped authority clock and relative-delay proof.
    PendingOnClock {
        /// Exact one-based attempt.
        attempt: u16,
        /// Durable epoch, origin, unit, and delay contract.
        schedule: ModelRetrySchedule,
    },
    /// The exact directive was claimed and attempt start committed.
    Running {
        /// Exact one-based attempt.
        attempt: u16,
        /// Positive caller monotonic tick at attempt start.
        started_at_tick: u64,
    },
    /// The exact directive started after authority-clock admission.
    RunningOnClock {
        /// Exact one-based attempt.
        attempt: u16,
        /// Authority-clock observation that admitted execution.
        started_at: AuthorityInstant,
        /// Whether the original epoch or an explicit restart rebase admitted it.
        basis: ModelStartBasis,
        /// Exact retry schedule admitted by the start, absent for immediate and legacy work.
        schedule: Option<ModelRetrySchedule>,
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
    pub const fn id(&self) -> ModelAnalysisId {
        self.id
    }
    /// Frozen plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// C5 semantic request digest.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }
    /// Frozen budget.
    #[must_use]
    pub const fn budget(&self) -> ModelBudget {
        self.budget
    }
    /// Current caller-owned retry policy.
    #[must_use]
    pub const fn retry_policy(&self) -> ModelRetryPolicy {
        self.retry_policy
    }
    /// Current work state.
    #[must_use]
    pub const fn state(&self) -> &ModelWorkState {
        &self.state
    }
    pub(crate) fn with_state(mut self, state: ModelWorkState) -> Self {
        self.state = state;
        self
    }
    pub(crate) fn with_retry_policy(mut self, retry_policy: ModelRetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }
}
