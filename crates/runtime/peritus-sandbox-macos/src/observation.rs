//! MacOS-specific bounded lifecycle and control observations.

use peritus_sandbox::{CapabilityDomain, SandboxResourceKind};
use peritus_types::Sha256Digest;

use crate::EnforcementLevel;

/// Closed macOS observation event vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationEvent {
    /// Preparation and manifest creation completed.
    Prepared,
    /// One native or supervisor-owned capability domain was mapped.
    ControlMapped,
    /// The process-group identity was accepted after helper launch.
    Activated,
    /// A resource dimension was mapped to an enforcement owner.
    ResourceMapped,
    /// Managed-proxy-only egress was mapped.
    ProxyMapped,
    /// Cancellation was accepted.
    CancelRequested,
    /// Root/helper termination was observed.
    Terminated,
    /// Every backend-owned resource was released.
    Released,
}

/// Outcome attached to one macOS observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationStatus {
    /// A control is installed or mapped.
    Enforced,
    /// The C2 supervisor owns enforcement for this fact.
    Supervised,
    /// A lifecycle operation completed.
    Completed,
    /// A cancellation request was accepted.
    Accepted,
    /// An idempotent cancellation or release had already completed.
    AlreadyComplete,
    /// Enforcement or cleanup could not be proven.
    Incomplete,
}

/// One ordered observation bound to every preparation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacosObservation {
    sequence: u64,
    plan_digest: Sha256Digest,
    descriptor_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    profile_digest: Sha256Digest,
    event: ObservationEvent,
    domain: Option<CapabilityDomain>,
    resource: Option<SandboxResourceKind>,
    enforcement: Option<EnforcementLevel>,
    status: ObservationStatus,
}

impl MacosObservation {
    /// Creates an exact bounded native observation.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "complete observation binding is intentional")]
    pub const fn new(
        sequence: u64,
        plan_digest: Sha256Digest,
        descriptor_digest: Sha256Digest,
        preparation_digest: Sha256Digest,
        profile_digest: Sha256Digest,
        event: ObservationEvent,
        domain: Option<CapabilityDomain>,
        resource: Option<SandboxResourceKind>,
        enforcement: Option<EnforcementLevel>,
        status: ObservationStatus,
    ) -> Self {
        Self {
            sequence,
            plan_digest,
            descriptor_digest,
            preparation_digest,
            profile_digest,
            event,
            domain,
            resource,
            enforcement,
            status,
        }
    }

    /// Returns the monotonic sequence number.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns the checked plan digest.
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest {
        self.plan_digest
    }

    /// Returns the admitted descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(self) -> Sha256Digest {
        self.descriptor_digest
    }

    /// Returns the admitted preparation digest.
    #[must_use]
    pub const fn preparation_digest(self) -> Sha256Digest {
        self.preparation_digest
    }

    /// Returns the compiled profile digest.
    #[must_use]
    pub const fn profile_digest(self) -> Sha256Digest {
        self.profile_digest
    }

    /// Returns the event kind.
    #[must_use]
    pub const fn event(self) -> ObservationEvent {
        self.event
    }

    /// Returns the affected C2 domain when applicable.
    #[must_use]
    pub const fn domain(self) -> Option<CapabilityDomain> {
        self.domain
    }

    /// Returns the affected resource dimension.
    #[must_use]
    pub const fn resource(self) -> Option<SandboxResourceKind> {
        self.resource
    }

    /// Returns the dimension-specific enforcement owner.
    #[must_use]
    pub const fn enforcement(self) -> Option<EnforcementLevel> {
        self.enforcement
    }

    /// Returns the observed status.
    #[must_use]
    pub const fn status(self) -> ObservationStatus {
        self.status
    }
}
