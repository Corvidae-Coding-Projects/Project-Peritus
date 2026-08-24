//! Ordered digest-bound native lifecycle and dimension-specific resource facts.

use peritus_sandbox::{
    CapabilityDomain, EnforcementObservation, ObservationDisposition, ObservationKind,
    SandboxResourceKind,
};
use peritus_types::Sha256Digest;

/// Native lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativePhase {
    /// Deterministic preparation and native resource installation completed.
    Prepared,
    /// Helper process was started and attached to containment.
    Activated,
    /// C2 accepted cancellation.
    CancelRequested,
    /// Root termination was observed.
    Terminated,
    /// Exact native resources were removed.
    Released,
}

/// Linux-specific capability facts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeCapability {
    /// Namespace/mount boundary.
    Namespaces,
    /// Landlock second filesystem layer.
    Landlock,
    /// Seccomp-BPF syscall policy.
    Seccomp,
    /// No-new-privileges and empty capability sets.
    PrivilegeDrop,
    /// Cgroup-v2 tree ownership.
    Cgroup,
    /// Pseudoterminal compatibility.
    Pty,
    /// Managed proxy-only route.
    ProxyRoute,
    /// One resource dimension.
    Resource(SandboxResourceKind),
}

/// Enforcement strength for a capability or resource dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EnforcementLevel {
    /// Kernel hard enforcement.
    Hard,
    /// C2 supervisor enforcement.
    Supervisor,
    /// Not available and therefore not advertised.
    Unsupported,
    /// Cleanup or observation is incomplete.
    Incomplete,
}

/// Observation result, never an acceptance or success claim.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationOutcome {
    /// Fact was installed or observed.
    Observed,
    /// An idempotent request was already observed.
    AlreadyObserved,
    /// Fact failed or became incomplete.
    Failed,
}

/// One rich Linux observation bound to every preparation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinuxObservation {
    sequence: u64,
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    probe_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    phase: NativePhase,
    capability: Option<NativeCapability>,
    enforcement: Option<EnforcementLevel>,
    outcome: ObservationOutcome,
}

impl LinuxObservation {
    /// Returns monotonic session sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Returns the checked sandbox plan identity.
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the selected backend identity.
    #[must_use]
    pub const fn backend_digest(self) -> Sha256Digest {
        self.backend_digest
    }
    /// Returns the exact runtime probe identity.
    #[must_use]
    pub const fn probe_digest(self) -> Sha256Digest {
        self.probe_digest
    }
    /// Returns the admitted preparation identity.
    #[must_use]
    pub const fn preparation_digest(self) -> Sha256Digest {
        self.preparation_digest
    }
    /// Returns the native phase.
    #[must_use]
    pub const fn phase(self) -> NativePhase {
        self.phase
    }
    /// Returns the probed capability.
    #[must_use]
    pub const fn capability(self) -> Option<NativeCapability> {
        self.capability
    }
    /// Returns enforcement strength.
    #[must_use]
    pub const fn enforcement(self) -> Option<EnforcementLevel> {
        self.enforcement
    }
    /// Returns the observation result.
    #[must_use]
    pub const fn outcome(self) -> ObservationOutcome {
        self.outcome
    }

    pub(crate) const fn new(
        sequence: u64,
        binding: ObservationBinding,
        phase: NativePhase,
        capability: Option<NativeCapability>,
        enforcement: Option<EnforcementLevel>,
        outcome: ObservationOutcome,
    ) -> Self {
        Self {
            sequence,
            plan_digest: binding.plan,
            backend_digest: binding.backend,
            probe_digest: binding.probe,
            preparation_digest: binding.preparation,
            phase,
            capability,
            enforcement,
            outcome,
        }
    }
}

/// Dimension-specific enforcement declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceEnforcement {
    kind: SandboxResourceKind,
    level: EnforcementLevel,
}

impl ResourceEnforcement {
    /// Creates one truthful dimension claim.
    #[must_use]
    pub const fn new(kind: SandboxResourceKind, level: EnforcementLevel) -> Self {
        Self { kind, level }
    }
    /// Returns the resource dimension.
    #[must_use]
    pub const fn kind(self) -> SandboxResourceKind {
        self.kind
    }
    /// Returns enforcement strength.
    #[must_use]
    pub const fn level(self) -> EnforcementLevel {
        self.level
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ObservationBinding {
    pub(super) plan: Sha256Digest,
    pub(super) backend: Sha256Digest,
    pub(super) probe: Sha256Digest,
    pub(super) preparation: Sha256Digest,
}

impl ObservationBinding {
    pub(crate) const fn common(
        self,
        sequence: u64,
        phase: NativePhase,
        outcome: ObservationOutcome,
    ) -> EnforcementObservation {
        EnforcementObservation::new(
            sequence,
            self.plan,
            self.backend,
            match phase {
                NativePhase::Prepared => ObservationKind::Prepared,
                NativePhase::Activated => ObservationKind::Activated,
                NativePhase::CancelRequested => ObservationKind::Cancellation,
                NativePhase::Terminated => ObservationKind::Terminated,
                NativePhase::Released => ObservationKind::Released,
            },
            phase_domain(phase),
            match outcome {
                ObservationOutcome::Observed => ObservationDisposition::Completed,
                ObservationOutcome::AlreadyObserved => ObservationDisposition::AlreadyAccepted,
                ObservationOutcome::Failed => ObservationDisposition::Failed,
            },
        )
    }
}

const fn phase_domain(phase: NativePhase) -> Option<CapabilityDomain> {
    match phase {
        NativePhase::Prepared | NativePhase::Released => None,
        NativePhase::Activated | NativePhase::CancelRequested | NativePhase::Terminated => {
            Some(CapabilityDomain::Process)
        }
    }
}
