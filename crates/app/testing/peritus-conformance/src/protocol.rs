//! Runtime-neutral A3 application-protocol conformance contract.

mod cases;

pub use cases::protocol_suite;

/// One independently exercised A3 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolScenario {
    /// Preferred version and every requested feature negotiate exactly.
    NegotiationExact,
    /// A common lower version or optional-feature subset is reported as downgraded.
    NegotiationDowngraded,
    /// Disjoint versions produce a typed incompatibility.
    NegotiationIncompatible,
    /// Missing required features cannot produce a session.
    RequiredFeature,
    /// Actor, correlation, revision, and exact B3 frames remain bound.
    CommandBinding,
    /// Same requests replay while changed key reuse conflicts.
    Idempotency,
    /// Resume, redelivery, and event-ID deduplication retain identity.
    SubscriptionResume,
    /// Cumulative acknowledgements cannot regress or exceed delivery.
    AckLegality,
    /// Retention gaps require an explicit snapshot decision.
    GapSnapshot,
    /// In-flight limits pause delivery without losing events.
    Backpressure,
    /// Artifact chunks conserve size, order, and final digest.
    ArtifactTransfer,
    /// Prompt answers retain exact correlation and freshness.
    PromptFreshness,
    /// Terminal output and exit ordering remain exact.
    TerminalOrdering,
    /// Readiness and shutdown controls report distinct truthful states.
    DaemonLifecycle,
    /// Malformed, truncated, trailing, and unknown wire input fails closed.
    MalformedInput,
    /// Independent protocol limits reject excess without truncation.
    Bounds,
}

/// Fixed realistic bounds supplied to one protocol case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolConformanceFixture {
    scenario: ProtocolScenario,
    maximum_features: u16,
    maximum_in_flight: u16,
    maximum_chunk_bytes: u32,
    maximum_frame_bytes: u32,
}

impl ProtocolConformanceFixture {
    pub(crate) const fn new(scenario: ProtocolScenario) -> Self {
        Self {
            scenario,
            maximum_features: 32,
            maximum_in_flight: 64,
            maximum_chunk_bytes: 1_048_576,
            maximum_frame_bytes: 16_777_216,
        }
    }

    /// Returns the selected behavior.
    #[must_use]
    pub const fn scenario(self) -> ProtocolScenario {
        self.scenario
    }

    /// Returns the feature-count ceiling.
    #[must_use]
    pub const fn maximum_features(self) -> u16 {
        self.maximum_features
    }

    /// Returns the unacknowledged-delivery ceiling.
    #[must_use]
    pub const fn maximum_in_flight(self) -> u16 {
        self.maximum_in_flight
    }

    /// Returns the artifact-chunk byte ceiling.
    #[must_use]
    pub const fn maximum_chunk_bytes(self) -> u32 {
        self.maximum_chunk_bytes
    }

    /// Returns the complete-frame byte ceiling.
    #[must_use]
    pub const fn maximum_frame_bytes(self) -> u32 {
        self.maximum_frame_bytes
    }
}

/// Direct observations from one complete A3 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent A3 contract facts remain visible to third-party implementations"
)]
pub struct ProtocolConformanceObservation {
    /// The selected scenario reached its expected typed terminal.
    pub expected_terminal: bool,
    /// Version/feature negotiation was deterministic and correctly classified.
    pub negotiation_exact: bool,
    /// Actor, correlation, revision, and exact B3 frame identity remained bound.
    pub command_binding_exact: bool,
    /// Idempotency replay/conflict/capacity behavior was exact.
    pub idempotency_exact: bool,
    /// Cursor, event, redelivery, and acknowledgement behavior was exact.
    pub delivery_exact: bool,
    /// Gap and backpressure recovery was explicit and lossless.
    pub flow_control_exact: bool,
    /// Artifact size, ordering, cancellation, and digest behavior was exact.
    pub artifact_exact: bool,
    /// Approval and user-input correlation/freshness was exact.
    pub prompt_exact: bool,
    /// Terminal output, input, resize, detach, cancellation, and exit ordering was exact.
    pub terminal_exact: bool,
    /// Readiness, diagnostics, heartbeat, and shutdown states remained distinct.
    pub daemon_control_exact: bool,
    /// Malformed or noncanonical input was rejected without a partial value.
    pub malformed_rejected: bool,
    /// Every configured independent limit was enforced without truncation.
    pub bounds_enforced: bool,
    /// Stable code, retryability, and subsystem were independent from prose.
    pub stable_error_exact: bool,
    /// Decoding and client intent never claimed authentication or durable authority.
    pub non_authoritative: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolConformanceError {
    /// The protocol boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by an A3 protocol subject.
pub trait ProtocolConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &ProtocolConformanceFixture,
    ) -> Result<ProtocolConformanceObservation, ProtocolConformanceError>;
}
