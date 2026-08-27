//! Runtime-neutral G0 daemon black-box conformance contract.

mod cases;
mod observation;

pub use cases::{daemon_scenario_suite, daemon_suite};
pub use observation::*;

/// One independently exercised daemon behavior from the G0 acceptance contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonScenario {
    /// A compatible authenticated client establishes or resumes its durable session.
    CompatibleSession,
    /// An incompatible hello establishes no session and changes no durable state.
    IncompatibleSession,
    /// A peer whose asserted actor differs from its durable binding is rejected.
    PeerActorMismatch,
    /// A post-hello frame with different negotiated context is rejected.
    ContextMismatch,
    /// A new idempotency key commits one exact command result.
    NewCommand,
    /// An exact idempotent retry returns the retained result without repeating work.
    ReplayCommand,
    /// Reuse of an idempotency key with a different request digest conflicts.
    ConflictingCommand,
    /// An ambiguous command is reconciled under its original identity.
    IndeterminateCommand,
    /// A command carrying a stale authority revision is rejected before effect.
    StaleRevision,
    /// A subscription resumes strictly after the supplied global source cursor.
    SubscriptionResume,
    /// An unacknowledged event is redelivered with stable event identity.
    SubscriptionRedelivery,
    /// A cumulative acknowledgement releases only its delivered prefix.
    SubscriptionAcknowledgement,
    /// A cursor before retained history requires an explicit snapshot.
    SubscriptionGap,
    /// A slow subscriber cannot exceed its negotiated in-flight bound.
    SubscriptionBackpressure,
    /// An immutable artifact is downloaded contiguously with exact identity.
    ArtifactDownload,
    /// An upload publishes the catalog only after exact finalization.
    ArtifactUpload,
    /// Corrupt artifact content is rejected without partial authority publication.
    ArtifactCorruption,
    /// Prompt settlement enforces actor, session, revision, and generation freshness.
    PromptFreshness,
    /// A terminal bridge preserves combined PTY order, offsets, and one exit.
    PtyOrdering,
    /// Read-only readiness admits observation while rejecting mutation and effects.
    ReadOnlyAdmission,
    /// A second live daemon leaves the active owner and endpoint untouched.
    SecondInstance,
    /// A diagnostic-safe startup failure publishes typed read-only readiness.
    StartupFailure,
    /// Effect-before-ack outbox recovery reconciles without duplicating the effect.
    OutboxCrash,
    /// Graceful shutdown drains every owned activity before reporting clean.
    GracefulShutdown,
    /// Forced restart reconciles durable work without orphaning or repeating it.
    ForcedRestart,
    /// Oversized work is rejected before allocation and retained state stays bounded.
    Bounds,
    /// Malformed framing is rejected before payload allocation or dispatch.
    MalformedFrame,
    /// Diagnostic and telemetry surfaces cannot exercise application authority.
    NonAuthority,
}

/// Complete closed G0 scenario inventory in contract order.
pub const DAEMON_SCENARIOS: &[DaemonScenario] = &[
    DaemonScenario::CompatibleSession,
    DaemonScenario::IncompatibleSession,
    DaemonScenario::PeerActorMismatch,
    DaemonScenario::ContextMismatch,
    DaemonScenario::NewCommand,
    DaemonScenario::ReplayCommand,
    DaemonScenario::ConflictingCommand,
    DaemonScenario::IndeterminateCommand,
    DaemonScenario::StaleRevision,
    DaemonScenario::SubscriptionResume,
    DaemonScenario::SubscriptionRedelivery,
    DaemonScenario::SubscriptionAcknowledgement,
    DaemonScenario::SubscriptionGap,
    DaemonScenario::SubscriptionBackpressure,
    DaemonScenario::ArtifactDownload,
    DaemonScenario::ArtifactUpload,
    DaemonScenario::ArtifactCorruption,
    DaemonScenario::PromptFreshness,
    DaemonScenario::PtyOrdering,
    DaemonScenario::ReadOnlyAdmission,
    DaemonScenario::SecondInstance,
    DaemonScenario::StartupFailure,
    DaemonScenario::OutboxCrash,
    DaemonScenario::GracefulShutdown,
    DaemonScenario::ForcedRestart,
    DaemonScenario::Bounds,
    DaemonScenario::MalformedFrame,
    DaemonScenario::NonAuthority,
];

/// Fixed portable values supplied to every black-box daemon case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonConformanceFixture {
    scenario: DaemonScenario,
    request_digest: [u8; 32],
    source_cursor: u64,
    authority_revision: u64,
    artifact_size: u64,
    maximum_frame_bytes: u64,
    maximum_in_flight: u64,
}

impl DaemonConformanceFixture {
    pub(crate) const fn new(scenario: DaemonScenario) -> Self {
        Self {
            scenario,
            request_digest: [0xb3; 32],
            source_cursor: 41,
            authority_revision: 7,
            artifact_size: 8_192,
            maximum_frame_bytes: 65_536,
            maximum_in_flight: 4,
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> DaemonScenario {
        self.scenario
    }

    /// Returns the exact request digest used for command replay and conflict cases.
    #[must_use]
    pub const fn request_digest(self) -> [u8; 32] {
        self.request_digest
    }

    /// Returns the subscription cursor from which delivery must resume.
    #[must_use]
    pub const fn source_cursor(self) -> u64 {
        self.source_cursor
    }

    /// Returns the current authority revision selected for the case.
    #[must_use]
    pub const fn authority_revision(self) -> u64 {
        self.authority_revision
    }

    /// Returns the exact artifact size used by transfer cases.
    #[must_use]
    pub const fn artifact_size(self) -> u64 {
        self.artifact_size
    }

    /// Returns the maximum accepted encoded frame size.
    #[must_use]
    pub const fn maximum_frame_bytes(self) -> u64 {
        self.maximum_frame_bytes
    }

    /// Returns the independent in-flight and retained-item ceiling.
    #[must_use]
    pub const fn maximum_in_flight(self) -> u64 {
        self.maximum_in_flight
    }
}

/// Stable failure classification returned by a production black-box adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonConformanceError {
    /// The isolated daemon subject could not be prepared.
    Setup,
    /// The protected local transport could not be exercised.
    Transport,
    /// The adapter could not collect the required direct observations.
    Observation,
}

/// Adapter implemented outside G0 internals against the production daemon boundary.
pub trait DaemonConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct black-box observations.
    ///
    /// Implementations must drive the same local protocol and process boundary used by production
    /// clients. Returning an observation is not itself a pass; the catalog evaluates every field.
    ///
    /// # Errors
    ///
    /// Returns a typed infrastructure failure when setup, transport, or observation is unavailable.
    fn exercise(
        &mut self,
        fixture: &DaemonConformanceFixture,
    ) -> Result<DaemonConformanceObservation, DaemonConformanceError>;
}
