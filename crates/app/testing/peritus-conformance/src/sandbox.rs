//! Runtime-neutral C2/C3 sandbox-policy and backend conformance contract.

mod cases;
mod fixtures;
mod observation;

pub use cases::sandbox_suite;
pub use observation::{SandboxConformanceObservation, SandboxPreparationObservation};

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxConformanceError {
    /// The sandbox boundary could not be exercised or observed.
    Infrastructure,
}

/// Complete sandbox capability domains.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxDomain {
    /// Filesystem discovery, metadata, read, execute, and mutation.
    Filesystem,
    /// Root process, descendants, signals, and containment.
    Process,
    /// Cleared or explicitly allowlisted environment.
    Environment,
    /// Outbound network rules and inbound denial.
    Network,
    /// Referenced secret delivery without value disclosure.
    Secret,
    /// Wall, CPU, memory, disk, output, handle, and process limits.
    Resource,
    /// Pipes, PTY, input, resize, and terminal signals.
    Terminal,
}

/// Stable enforcement features used by preparation fixtures.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxFeature {
    /// Read-only filesystem access.
    FilesystemRead,
    /// Filesystem write control.
    FilesystemWrite,
    /// Explicit descendant spawning.
    Descendants,
    /// Literal environment injection into a cleared environment.
    EnvironmentLiteral,
    /// Exact outbound network allow rules.
    NetworkOutbound,
    /// Secret delivery through an environment destination.
    SecretEnvironment,
    /// Wall-time accounting and enforcement.
    WallTime,
    /// Output-byte accounting and enforcement.
    OutputBytes,
    /// Pseudo-terminal allocation.
    Pty,
    /// Pseudo-terminal resize control.
    Resize,
    /// Complete process-tree containment.
    TreeContainment,
}

/// One independently exercised sandbox behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxScenario {
    /// A contract with no grants denies every domain.
    DefaultDeny,
    /// A filesystem deny rule dominates an overlapping allow rule.
    FilesystemDenyDominance,
    /// Explicit environment and secret-reference delivery is exercised.
    EnvironmentSecret,
    /// One exact outbound destination is allowed.
    NetworkAllowed,
    /// An undeclared outbound destination is denied.
    NetworkDenied,
    /// Descendants and PTY controls remain within the contract.
    ProcessTerminalWithin,
    /// A descendant or terminal request exceeds the contract.
    ProcessTerminalExceeded,
    /// Resource consumption equals its ceiling.
    ResourceAtLimit,
    /// Resource consumption is one unit over its ceiling.
    ResourceOverLimit,
    /// A required backend feature is absent.
    Unsupported,
    /// Cancellation reaches release with complete teardown.
    Cancellation,
    /// Ordered observations bind the exact prepared plan.
    ObservationBinding,
}

/// Stable sandbox decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxDecision {
    /// The exact request is within the contract.
    Allowed,
    /// A deny or absent grant rejects the request.
    Denied,
    /// The backend cannot enforce a required feature.
    Unsupported,
    /// Exact resource or lifecycle policy was exceeded.
    Violation,
    /// Cancellation won the lifecycle outcome.
    Cancelled,
}

/// Reference/native backend lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxLifecyclePhase {
    /// Plan exists but preparation has not started.
    Planned,
    /// Backend preparation completed.
    Prepared,
    /// Enforcement session is active.
    Active,
    /// Cancellation is in progress.
    Cancelling,
    /// Work terminated and teardown is pending.
    Terminated,
    /// All enforcement resources were released.
    Released,
}

/// Complete fixed request supplied by one sandbox behavior case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxConformanceFixture {
    scenario: SandboxScenario,
    filesystem_path: &'static str,
    environment_name: &'static str,
    network_host: &'static str,
    network_port: u16,
    secret_reference: &'static str,
    secret_canary: &'static [u8],
    resource_limit: u64,
    resource_requested: u64,
}

impl SandboxConformanceFixture {
    #[allow(clippy::too_many_arguments, reason = "the cross-domain fixture remains explicit")]
    pub(crate) const fn new(
        scenario: SandboxScenario,
        filesystem_path: &'static str,
        environment_name: &'static str,
        network_host: &'static str,
        network_port: u16,
        secret_reference: &'static str,
        secret_canary: &'static [u8],
        resource_limit: u64,
        resource_requested: u64,
    ) -> Self {
        Self {
            scenario,
            filesystem_path,
            environment_name,
            network_host,
            network_port,
            secret_reference,
            secret_canary,
            resource_limit,
            resource_requested,
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(&self) -> SandboxScenario {
        self.scenario
    }
    /// Returns the exact filesystem probe path.
    #[must_use]
    pub const fn filesystem_path(&self) -> &'static str {
        self.filesystem_path
    }
    /// Returns the exact environment destination.
    #[must_use]
    pub const fn environment_name(&self) -> &'static str {
        self.environment_name
    }
    /// Returns the requested network host.
    #[must_use]
    pub const fn network_host(&self) -> &'static str {
        self.network_host
    }
    /// Returns the requested network port.
    #[must_use]
    pub const fn network_port(&self) -> u16 {
        self.network_port
    }
    /// Returns the opaque secret reference.
    #[must_use]
    pub const fn secret_reference(&self) -> &'static str {
        self.secret_reference
    }
    /// Returns the canary used only to detect secret disclosure.
    #[must_use]
    pub const fn secret_canary(&self) -> &'static [u8] {
        self.secret_canary
    }
    /// Returns the exact resource ceiling.
    #[must_use]
    pub const fn resource_limit(&self) -> u64 {
        self.resource_limit
    }
    /// Returns the requested resource amount.
    #[must_use]
    pub const fn resource_requested(&self) -> u64 {
        self.resource_requested
    }
}

/// Semantically equivalent or deliberately drifted preparation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxPreparationFixture {
    required_features: &'static [SandboxFeature],
    backend_features: &'static [SandboxFeature],
    secret_canary: &'static [u8],
    authority_marker: u64,
}

impl SandboxPreparationFixture {
    pub(crate) const fn new(
        required_features: &'static [SandboxFeature],
        backend_features: &'static [SandboxFeature],
        secret_canary: &'static [u8],
        authority_marker: u64,
    ) -> Self {
        Self { required_features, backend_features, secret_canary, authority_marker }
    }

    /// Returns required features in caller-supplied order.
    #[must_use]
    pub const fn required_features(&self) -> &'static [SandboxFeature] {
        self.required_features
    }
    /// Returns backend features in caller-supplied order.
    #[must_use]
    pub const fn backend_features(&self) -> &'static [SandboxFeature] {
        self.backend_features
    }
    /// Returns the canary used only to detect secret disclosure.
    #[must_use]
    pub const fn secret_canary(&self) -> &'static [u8] {
        self.secret_canary
    }
    /// Returns one authority-relevant field used by the drift check.
    #[must_use]
    pub const fn authority_marker(&self) -> u64 {
        self.authority_marker
    }
}

/// Adapter implemented by a C2 reference or C3 native sandbox under conformance test.
pub trait SandboxConformanceSubject: Send {
    /// Exercises one complete cross-domain request and returns raw observations.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxConformanceError::Infrastructure`] when execution cannot be observed.
    fn exercise(
        &mut self,
        fixture: &SandboxConformanceFixture,
    ) -> Result<SandboxConformanceObservation, SandboxConformanceError>;

    /// Compiles and prepares inert policy without launching a native effect.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxConformanceError::Infrastructure`] when preparation cannot be observed.
    fn prepare(
        &mut self,
        fixture: &SandboxPreparationFixture,
    ) -> Result<SandboxPreparationObservation, SandboxConformanceError>;
}
