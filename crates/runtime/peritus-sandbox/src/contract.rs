//! Aggregate sandbox capability contract.

use crate::{
    EnvironmentContract, FilesystemContract, NetworkContract, ProcessContract, ResourceLimits,
    SecretContract, TerminalContract,
};

/// Requested isolation strength.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IsolationRequirement {
    /// Every requested effect must be enforced by a native production backend.
    Restricted,
    /// The caller explicitly selected raw-effect execution; contracts remain observable.
    ExplicitRawEffect,
}

impl IsolationRequirement {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Restricted => 0,
            Self::ExplicitRawEffect => 1,
        }
    }
}

/// Local sandbox operation class mapped by the process-owned authorization boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxOperationClass {
    /// Restricted process execution.
    Execution,
    /// Explicitly authorized raw-effect execution.
    RawEffect,
}

impl SandboxOperationClass {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Execution => 0,
            Self::RawEffect => 1,
        }
    }
}

/// Complete seven-domain sandbox contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxContract {
    filesystem: FilesystemContract,
    process: ProcessContract,
    environment: EnvironmentContract,
    network: NetworkContract,
    secrets: SecretContract,
    resources: ResourceLimits,
    terminal: TerminalContract,
}

impl SandboxContract {
    /// Creates a complete contract. Domain constructors have already validated their values.
    #[must_use]
    pub const fn new(
        filesystem: FilesystemContract,
        process: ProcessContract,
        environment: EnvironmentContract,
        network: NetworkContract,
        secrets: SecretContract,
        resources: ResourceLimits,
        terminal: TerminalContract,
    ) -> Self {
        Self { filesystem, process, environment, network, secrets, resources, terminal }
    }
    /// Returns filesystem policy.
    #[must_use]
    pub const fn filesystem(&self) -> &FilesystemContract {
        &self.filesystem
    }
    /// Returns process policy.
    #[must_use]
    pub const fn process(&self) -> &ProcessContract {
        &self.process
    }
    /// Returns environment policy.
    #[must_use]
    pub const fn environment(&self) -> &EnvironmentContract {
        &self.environment
    }
    /// Returns network policy.
    #[must_use]
    pub const fn network(&self) -> &NetworkContract {
        &self.network
    }
    /// Returns secret policy.
    #[must_use]
    pub const fn secrets(&self) -> &SecretContract {
        &self.secrets
    }
    /// Returns resource policy.
    #[must_use]
    pub const fn resources(&self) -> &ResourceLimits {
        &self.resources
    }
    /// Returns terminal policy.
    #[must_use]
    pub const fn terminal(&self) -> TerminalContract {
        self.terminal
    }
}
