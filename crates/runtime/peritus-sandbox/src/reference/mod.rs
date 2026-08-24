//! Deterministic executable backend for unit and cross-platform conformance tests.

mod accounting;
mod evaluation;
mod session;

pub use session::{ReferenceSession, ResourceDecision, TerminationKind};

use crate::{
    BackendAdmission, BackendDescriptor, BackendKind, BackendName, BackendVersion,
    CheckedSandboxPlan, EnvironmentName, FeatureSet, FileRequirement, NetworkTarget, PathSemantics,
    RequestedTerminalOperation, ResourceFidelity, SandboxError, SandboxOperation, SandboxPath,
    SecretRequirement,
};

const REFERENCE_BACKEND_VERSION: &str = "0.0.0";

/// Process signal requested by a reference probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestedProcessSignal {
    /// Request graceful termination.
    Graceful,
    /// Request forced termination.
    Forced,
}

/// One semantic operation evaluated without operating-system effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReferenceProbe {
    /// Filesystem path operation.
    Filesystem(FileRequirement),
    /// Root program selection.
    RootProgram(SandboxPath),
    /// Simultaneous descendant count.
    DescendantCount(u32),
    /// Process signal request.
    ProcessSignal(RequestedProcessSignal),
    /// Host environment inheritance.
    InheritedEnvironment(EnvironmentName),
    /// Literal environment assignment.
    LiteralEnvironment(EnvironmentName),
    /// Network connection.
    Network(NetworkTarget),
    /// Exact secret delivery.
    Secret(SecretRequirement),
    /// Terminal operation.
    Terminal(RequestedTerminalOperation),
}

/// Result of semantic capability evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeDecision {
    /// Contract permits the operation.
    Allowed,
    /// Contract denies the operation.
    Denied,
}

/// Deterministic reference-backend fault point.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReferenceFault {
    /// Backend support admission fails.
    Support,
    /// Preparation fails.
    Prepare,
    /// Activation fails.
    Activate,
    /// Capability evaluation fails.
    Evaluate,
    /// Resource accounting fails.
    Account,
    /// Cancellation fails.
    Cancel,
    /// Termination fails.
    Terminate,
    /// Release fails.
    Release,
    /// Recording the next observation fails.
    Observation,
}

impl ReferenceFault {
    const fn bit(self) -> u16 {
        match self {
            Self::Support => 1 << 0,
            Self::Prepare => 1 << 1,
            Self::Activate => 1 << 2,
            Self::Evaluate => 1 << 3,
            Self::Account => 1 << 4,
            Self::Cancel => 1 << 5,
            Self::Terminate => 1 << 6,
            Self::Release => 1 << 7,
            Self::Observation => 1 << 8,
        }
    }
}

/// Set of deterministic faults enabled for a reference session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReferenceFaultPlan(u16);

impl ReferenceFaultPlan {
    /// Returns a fault-free plan.
    #[must_use]
    pub const fn none() -> Self {
        Self(0)
    }
    /// Creates a plan containing each supplied fault point.
    #[must_use]
    pub fn from_faults(faults: impl IntoIterator<Item = ReferenceFault>) -> Self {
        let mut plan = Self::none();
        for fault in faults {
            plan.0 |= fault.bit();
        }
        plan
    }
    /// Reports whether a fault is enabled.
    #[must_use]
    pub const fn contains(self, fault: ReferenceFault) -> bool {
        self.0 & fault.bit() != 0
    }
}

/// Platform-independent executable backend with no operating-system effects.
#[derive(Clone, Debug)]
pub struct ReferenceBackend {
    descriptor: BackendDescriptor,
    faults: ReferenceFaultPlan,
    observation_limit: usize,
}

impl ReferenceBackend {
    /// Default maximum observations retained by one session.
    pub const DEFAULT_OBSERVATION_LIMIT: usize = 4_096;
    const REQUIRED_LIFECYCLE_OBSERVATIONS: usize = 5;

    /// Creates a reference backend with complete semantic support.
    ///
    /// # Errors
    /// Returns an input error when `observation_limit` cannot retain the five lifecycle events.
    pub fn new(faults: ReferenceFaultPlan, observation_limit: usize) -> Result<Self, SandboxError> {
        if observation_limit < Self::REQUIRED_LIFECYCLE_OBSERVATIONS {
            return Err(crate::error::invalid("observation limit cannot retain lifecycle"));
        }
        let name = BackendName::new("peritus-reference")?;
        let version = BackendVersion::new(REFERENCE_BACKEND_VERSION)?;
        let descriptor = BackendDescriptor::new(
            name,
            version,
            BackendKind::ReferenceOnly,
            PathSemantics::LogicalUtf8,
            ResourceFidelity::Reference,
            FeatureSet::all(),
        );
        Ok(Self { descriptor, faults, observation_limit })
    }

    /// Admits this backend for one checked plan.
    ///
    /// # Errors
    /// Returns an injected support fault or the ordinary fail-closed admission error.
    pub fn admit(
        &self,
        plan: &CheckedSandboxPlan,
        profile: crate::AdmissionProfile,
    ) -> Result<BackendAdmission, SandboxError> {
        if self.faults.contains(ReferenceFault::Support) {
            return Err(crate::error::injected(SandboxOperation::AdmitBackend));
        }
        crate::admit_backend(plan, &self.descriptor, profile)
    }

    /// Creates the ordinary fault-free backend.
    ///
    /// # Panics
    /// Panics only if a crate-owned backend identity or the nonzero built-in observation bound is
    /// changed to an invalid value; crate tests guard those constants.
    #[must_use]
    pub fn fault_free() -> Self {
        Self::new(ReferenceFaultPlan::none(), Self::DEFAULT_OBSERVATION_LIMIT)
            .expect("built-in reference descriptor and limit are valid")
    }
}

impl Default for ReferenceBackend {
    fn default() -> Self {
        Self::fault_free()
    }
}

impl crate::SandboxPreparation for ReferenceBackend {
    type Prepared = ReferenceSession;

    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn prepare(
        &self,
        plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<Self::Prepared, SandboxError> {
        if self.faults.contains(ReferenceFault::Prepare) {
            return Err(crate::error::injected(SandboxOperation::Prepare));
        }
        if admission.plan_digest() != plan.digest()
            || admission.descriptor_digest() != self.descriptor.digest()
            || admission.preparation_digest()
                != crate::canonical::preparation_digest(
                    plan.digest(),
                    self.descriptor.digest(),
                    self.descriptor.support_digest(),
                )
        {
            return Err(SandboxError::new(
                crate::SandboxErrorKind::BackendMismatch,
                SandboxOperation::Prepare,
                crate::RecoveryClass::Replan,
                "plan, admission, and backend identity disagree",
            ));
        }
        let contract_limit =
            usize::try_from(plan.contract().terminal().limits().event_count().get())
                .unwrap_or(usize::MAX);
        ReferenceSession::new(
            plan.clone(),
            self.descriptor.clone(),
            self.faults,
            self.observation_limit.min(contract_limit),
        )
    }
}
