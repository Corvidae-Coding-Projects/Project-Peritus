//! Bounded backend enforcement observations.

use peritus_types::Sha256Digest;

/// Sandbox capability domain associated with an observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CapabilityDomain {
    /// Filesystem.
    Filesystem,
    /// Process tree.
    Process,
    /// Environment.
    Environment,
    /// Network.
    Network,
    /// Secret delivery.
    Secret,
    /// Resource accounting.
    Resource,
    /// Terminal.
    Terminal,
}

/// Closed observation event vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationKind {
    /// Backend preparation completed.
    Prepared,
    /// Session activation completed.
    Activated,
    /// One capability probe was evaluated.
    CapabilityEvaluated,
    /// Resource usage was charged.
    ResourceCharged,
    /// Cancellation was accepted or observed again.
    Cancellation,
    /// A terminal result was recorded.
    Terminated,
    /// Backend state was released.
    Released,
    /// A deterministic injected fault fired.
    FaultInjected,
}

/// Outcome carried by an observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationDisposition {
    /// Requested capability was permitted.
    Allowed,
    /// Requested capability was denied.
    Denied,
    /// Lifecycle operation completed.
    Completed,
    /// An operation was accepted for processing.
    Accepted,
    /// Operation was already accepted and remained idempotent.
    AlreadyAccepted,
    /// Operation failed.
    Failed,
}

/// One ordered observation bound to a plan and backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnforcementObservation {
    sequence: u64,
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    kind: ObservationKind,
    domain: Option<CapabilityDomain>,
    disposition: ObservationDisposition,
}

impl EnforcementObservation {
    /// Creates a fully bound observation.
    #[must_use]
    pub const fn new(
        sequence: u64,
        plan_digest: Sha256Digest,
        backend_digest: Sha256Digest,
        kind: ObservationKind,
        domain: Option<CapabilityDomain>,
        disposition: ObservationDisposition,
    ) -> Self {
        Self { sequence, plan_digest, backend_digest, kind, domain, disposition }
    }
    /// Returns the monotonic session sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Returns plan identity.
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns backend identity.
    #[must_use]
    pub const fn backend_digest(self) -> Sha256Digest {
        self.backend_digest
    }
    /// Returns event kind.
    #[must_use]
    pub const fn kind(self) -> ObservationKind {
        self.kind
    }
    /// Returns affected domain.
    #[must_use]
    pub const fn domain(self) -> Option<CapabilityDomain> {
        self.domain
    }
    /// Returns event outcome.
    #[must_use]
    pub const fn disposition(self) -> ObservationDisposition {
        self.disposition
    }
}

/// Whether termination and release observations prove teardown completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeardownCompleteness {
    /// Termination and release were both observed in order.
    Complete,
    /// At least one required terminal observation is absent.
    Incomplete,
}

/// Computes teardown completeness from an observation stream.
#[must_use]
pub fn teardown_completeness(observations: &[EnforcementObservation]) -> TeardownCompleteness {
    let terminated =
        observations.iter().position(|event| event.kind == ObservationKind::Terminated);
    let released = observations.iter().position(|event| event.kind == ObservationKind::Released);
    if matches!((terminated, released), (Some(left), Some(right)) if left < right) {
        TeardownCompleteness::Complete
    } else {
        TeardownCompleteness::Incomplete
    }
}
