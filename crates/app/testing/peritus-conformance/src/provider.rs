//! Runtime-neutral model-provider conformance contract.

mod cases;
mod fixtures;
mod observation;

pub use cases::provider_suite;
pub use observation::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderCancellationObservation,
    ProviderCapabilityObservation, ProviderConformanceObservation, ProviderEventKind,
    ProviderEventObservation, ProviderFailureObservation, ProviderIsolationObservation,
    ProviderRedactionObservation, ProviderRetryObservation, ProviderStreamObservation,
    ProviderUsageObservation, ProviderUsageSnapshot,
};

/// One independently exercised model-provider behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderScenario {
    /// Advertised features work and unsupported features fail before transport.
    CapabilityHonesty,
    /// Provider ordering is retained and, when the dialect exposes event identity, only exact
    /// duplicates are ignored.
    OrderedDeduplication,
    /// Tool arguments split across chunks complete only after their close event.
    FragmentedToolCall,
    /// Malformed provider data cannot become a successful response.
    MalformedPayload,
    /// End-of-stream without a terminal event fails closed.
    IncompleteStream,
    /// A transport interruption remains an explicit failure.
    Interruption,
    /// Cancellation interrupts pending work and joins its owner.
    Cancellation,
    /// Authentication rejection remains typed and non-retryable.
    AuthenticationFailure,
    /// Rate limiting honors the bounded provider retry-after observation.
    RateLimitRetryAfter,
    /// A transient failure follows the bounded retry plan.
    TransientRetry,
    /// A maybe-accepted submission is never blindly recreated.
    AmbiguousSubmission,
    /// Usage snapshots are monotonic and retain exact final counters.
    UsageAccounting,
    /// Sensitive canaries are absent from all reportable surfaces.
    Redaction,
    /// Configuration, credentials, and transport remain adapter-instance local.
    AdapterIsolation,
}

/// Portable capabilities varied by the provider suite.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProviderCapability {
    /// Incremental response streaming.
    Streaming,
    /// Application function calls.
    ToolCalls,
    /// Concurrent function calls.
    ParallelToolCalls,
    /// Strict schema-constrained output.
    StructuredOutput,
    /// Model reasoning controls or replay state.
    Reasoning,
    /// Image input.
    ImageInput,
    /// Audio input.
    AudioInput,
    /// Provider prompt caching.
    PromptCaching,
    /// Exact provider cursor resumption.
    ExactResume,
    /// Provider-confirmed cancellation.
    ConfirmedCancellation,
    /// Detailed usage observations.
    UsageDetail,
}

/// Stable terminal class directly observed from one exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderTerminal {
    /// One valid success terminal was reduced.
    Completed,
    /// One explicit failure terminal was reduced.
    Failed,
    /// One explicit cancellation terminal was reduced.
    Cancelled,
}

/// Stable provider failure class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderFailureKind {
    /// Provider bytes or event grammar were malformed.
    Malformed,
    /// Transport ended without a valid terminal.
    Incomplete,
    /// Transport failed after events became observable.
    Interrupted,
    /// Provider rejected authentication or authorization.
    Authentication,
}

/// Complete fixed request supplied by one provider conformance case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderConformanceFixture {
    scenario: ProviderScenario,
    expected_tool_arguments_digest: [u8; 32],
    retry_after_millis: u64,
    max_retry_delay_millis: u64,
    canary: &'static str,
    selected_adapter: &'static str,
    foreign_adapter: &'static str,
}

impl ProviderConformanceFixture {
    pub(crate) const fn new(scenario: ProviderScenario) -> Self {
        Self {
            scenario,
            expected_tool_arguments_digest: [0x5a; 32],
            retry_after_millis: 250,
            max_retry_delay_millis: 5_000,
            canary: "peritus-provider-secret-canary",
            selected_adapter: "provider-primary",
            foreign_adapter: "provider-foreign",
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(&self) -> ProviderScenario {
        self.scenario
    }

    /// Returns the expected digest of the completed tool arguments.
    #[must_use]
    pub const fn expected_tool_arguments_digest(&self) -> [u8; 32] {
        self.expected_tool_arguments_digest
    }

    /// Returns the exact scripted rate-limit delay.
    #[must_use]
    pub const fn retry_after_millis(&self) -> u64 {
        self.retry_after_millis
    }

    /// Returns the maximum accepted transient retry delay.
    #[must_use]
    pub const fn max_retry_delay_millis(&self) -> u64 {
        self.max_retry_delay_millis
    }

    /// Returns the sensitive redaction canary.
    #[must_use]
    pub const fn canary(&self) -> &'static str {
        self.canary
    }

    /// Returns the adapter instance selected for the request.
    #[must_use]
    pub const fn selected_adapter(&self) -> &'static str {
        self.selected_adapter
    }

    /// Returns the adapter instance that must remain untouched.
    #[must_use]
    pub const fn foreign_adapter(&self) -> &'static str {
        self.foreign_adapter
    }
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderConformanceError {
    /// The provider boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by a production provider's development-only conformance bridge.
pub trait ProviderConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations rather than a claimed verdict.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &ProviderConformanceFixture,
    ) -> Result<ProviderConformanceObservation, ProviderConformanceError>;
}
