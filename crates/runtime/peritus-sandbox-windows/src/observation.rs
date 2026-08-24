//! Bounded Windows-native lifecycle and enforcement observations.

use peritus_sandbox::{
    CapabilityDomain, EnforcementObservation, ObservationDisposition, ObservationKind,
    SandboxResourceKind,
};
use peritus_types::Sha256Digest;

use crate::{EnforcementLevel, WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

/// Windows session lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WindowsPhase {
    /// Deterministic preparation and temporary ACL installation completed.
    Prepared,
    /// Helper installed native controls and target containment.
    Activated,
    /// The first cancellation request was accepted.
    CancelRequested,
    /// Root/helper termination was observed.
    Terminated,
    /// Every backend-owned resource was released.
    Released,
}

impl WindowsPhase {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::Activated => 1,
            Self::CancelRequested => 2,
            Self::Terminated => 3,
            Self::Released => 4,
        }
    }

    pub(crate) const fn from_ordinal(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Prepared),
            1 => Some(Self::Activated),
            2 => Some(Self::CancelRequested),
            3 => Some(Self::Terminated),
            4 => Some(Self::Released),
            _ => None,
        }
    }
}

/// Windows control named by a rich observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WindowsCapability {
    /// Restricted primary token.
    RestrictedToken,
    /// Low mandatory integrity.
    LowIntegrity,
    /// `AppContainer` isolation.
    AppContainer,
    /// Kill-on-close Job Object.
    JobObject,
    /// Exact temporary ACL plan.
    Acl,
    /// Reparse/volume/path validation.
    PathResolution,
    /// Closed inherited-handle list.
    HandleList,
    /// C2-owned `ConPTY` mapping.
    ConPty,
    /// Deny-all or managed-filter network isolation.
    Network,
    /// Protected secret handles.
    SecretHandles,
}

/// Rich observation result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationStatus {
    /// Native control was installed.
    Installed,
    /// An exact native fact was verified.
    Verified,
    /// The operation was explicitly denied.
    Denied,
    /// Teardown or observation is incomplete.
    Incomplete,
}

/// Complete digest binding shared by observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationBinding {
    plan: Sha256Digest,
    backend: Sha256Digest,
    probe: Sha256Digest,
    preparation: Sha256Digest,
}

impl ObservationBinding {
    /// Creates exact plan/backend/probe/preparation binding.
    #[must_use]
    pub const fn new(
        plan: Sha256Digest,
        backend: Sha256Digest,
        probe: Sha256Digest,
        preparation: Sha256Digest,
    ) -> Self {
        Self { plan, backend, probe, preparation }
    }

    pub(crate) const fn common(
        self,
        sequence: u64,
        phase: WindowsPhase,
        disposition: ObservationDisposition,
    ) -> EnforcementObservation {
        let kind = match phase {
            WindowsPhase::Prepared => ObservationKind::Prepared,
            WindowsPhase::Activated => ObservationKind::Activated,
            WindowsPhase::CancelRequested => ObservationKind::Cancellation,
            WindowsPhase::Terminated => ObservationKind::Terminated,
            WindowsPhase::Released => ObservationKind::Released,
        };
        EnforcementObservation::new(sequence, self.plan, self.backend, kind, None, disposition)
    }

    /// Returns checked sandbox identity.
    #[must_use]
    pub const fn plan(self) -> Sha256Digest {
        self.plan
    }
    /// Returns backend descriptor identity.
    #[must_use]
    pub const fn backend(self) -> Sha256Digest {
        self.backend
    }
    /// Returns full runtime probe identity.
    #[must_use]
    pub const fn probe(self) -> Sha256Digest {
        self.probe
    }
    /// Returns admitted preparation identity.
    #[must_use]
    pub const fn preparation(self) -> Sha256Digest {
        self.preparation
    }
}

/// One bounded, fully bound Windows-specific fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsObservation {
    sequence: u64,
    binding: ObservationBinding,
    phase: WindowsPhase,
    capability: Option<WindowsCapability>,
    resource: Option<SandboxResourceKind>,
    enforcement: Option<EnforcementLevel>,
    status: ObservationStatus,
}

impl WindowsObservation {
    /// Creates one fully bound rich observation.
    #[allow(clippy::too_many_arguments, reason = "closed observation schema")]
    #[must_use]
    pub const fn new(
        sequence: u64,
        binding: ObservationBinding,
        phase: WindowsPhase,
        capability: Option<WindowsCapability>,
        resource: Option<SandboxResourceKind>,
        enforcement: Option<EnforcementLevel>,
        status: ObservationStatus,
    ) -> Self {
        Self { sequence, binding, phase, capability, resource, enforcement, status }
    }

    /// Returns monotonic rich sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Returns full digest binding.
    #[must_use]
    pub const fn binding(self) -> ObservationBinding {
        self.binding
    }
    /// Returns lifecycle phase.
    #[must_use]
    pub const fn phase(self) -> WindowsPhase {
        self.phase
    }
    /// Returns optional control.
    #[must_use]
    pub const fn capability(self) -> Option<WindowsCapability> {
        self.capability
    }
    /// Returns optional resource dimension.
    #[must_use]
    pub const fn resource(self) -> Option<SandboxResourceKind> {
        self.resource
    }
    /// Returns optional enforcement owner.
    #[must_use]
    pub const fn enforcement(self) -> Option<EnforcementLevel> {
        self.enforcement
    }
    /// Returns observation result.
    #[must_use]
    pub const fn status(self) -> ObservationStatus {
        self.status
    }
}

pub(crate) const fn transition_allowed(current: WindowsPhase, next: WindowsPhase) -> bool {
    crate::verified::lifecycle_transition_allowed(current.ordinal(), next.ordinal())
}

pub(crate) fn observation_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Observation,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}

/// Maps one resource dimension to the common C2 observation domain.
#[must_use]
pub const fn resource_domain(_kind: SandboxResourceKind) -> CapabilityDomain {
    CapabilityDomain::Resource
}
