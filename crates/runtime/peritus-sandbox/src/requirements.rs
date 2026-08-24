//! Concrete effects required by one planned invocation.

use crate::{
    EnvironmentName, FileOperation, NetworkTarget, ProcessRequirements, ResourceLimits,
    SandboxError, SandboxPath, SecretRequirement, TerminalRequirements,
};

const MAX_REQUIREMENTS: usize = 256;

/// One required filesystem operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileRequirement {
    path: SandboxPath,
    operation: FileOperation,
}

impl FileRequirement {
    /// Creates a filesystem requirement.
    #[must_use]
    pub const fn new(path: SandboxPath, operation: FileOperation) -> Self {
        Self { path, operation }
    }
    /// Returns the path.
    #[must_use]
    pub const fn path(&self) -> &SandboxPath {
        &self.path
    }
    /// Returns the operation.
    #[must_use]
    pub const fn operation(&self) -> FileOperation {
        self.operation
    }
}

/// Environment names needed from inheritance or literal assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentRequirements {
    inherited_names: Vec<EnvironmentName>,
    literal_names: Vec<EnvironmentName>,
}

impl EnvironmentRequirements {
    /// Validates and canonicalizes environment requirements.
    ///
    /// # Errors
    /// Returns a limit error for more than 256 names in either category.
    pub fn new(
        mut inherited_names: Vec<EnvironmentName>,
        mut literal_names: Vec<EnvironmentName>,
    ) -> Result<Self, SandboxError> {
        if inherited_names.len() > MAX_REQUIREMENTS || literal_names.len() > MAX_REQUIREMENTS {
            return Err(crate::error::bound("too many environment requirements"));
        }
        inherited_names.sort();
        inherited_names.dedup();
        literal_names.sort();
        literal_names.dedup();
        Ok(Self { inherited_names, literal_names })
    }
    /// Returns canonical inherited names.
    #[must_use]
    pub fn inherited_names(&self) -> &[EnvironmentName] {
        &self.inherited_names
    }
    /// Returns canonical literal names.
    #[must_use]
    pub fn literal_names(&self) -> &[EnvironmentName] {
        &self.literal_names
    }
}

/// Complete concrete requirements checked against a sandbox contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxRequirements {
    files: Vec<FileRequirement>,
    process: ProcessRequirements,
    environment: EnvironmentRequirements,
    network: Vec<NetworkTarget>,
    secrets: Vec<SecretRequirement>,
    resources: ResourceLimits,
    terminal: TerminalRequirements,
}

impl SandboxRequirements {
    /// Validates and canonicalizes all collection-valued requirements.
    ///
    /// # Errors
    /// Returns a limit error when a collection exceeds 256 entries.
    #[allow(clippy::too_many_arguments, reason = "one value per closed sandbox domain")]
    pub fn new(
        mut files: Vec<FileRequirement>,
        process: ProcessRequirements,
        environment: EnvironmentRequirements,
        mut network: Vec<NetworkTarget>,
        mut secrets: Vec<SecretRequirement>,
        resources: ResourceLimits,
        terminal: TerminalRequirements,
    ) -> Result<Self, SandboxError> {
        if files.len() > MAX_REQUIREMENTS
            || network.len() > MAX_REQUIREMENTS
            || secrets.len() > MAX_REQUIREMENTS
        {
            return Err(crate::error::bound("too many sandbox requirements"));
        }
        files.sort();
        files.dedup();
        network.sort();
        network.dedup();
        secrets.sort();
        secrets.dedup();
        Ok(Self { files, process, environment, network, secrets, resources, terminal })
    }
    /// Returns canonical filesystem requirements.
    #[must_use]
    pub fn files(&self) -> &[FileRequirement] {
        &self.files
    }
    /// Returns process requirements.
    #[must_use]
    pub const fn process(&self) -> &ProcessRequirements {
        &self.process
    }
    /// Returns environment requirements.
    #[must_use]
    pub const fn environment(&self) -> &EnvironmentRequirements {
        &self.environment
    }
    /// Returns canonical network requirements.
    #[must_use]
    pub fn network(&self) -> &[NetworkTarget] {
        &self.network
    }
    /// Returns canonical secret requirements.
    #[must_use]
    pub fn secrets(&self) -> &[SecretRequirement] {
        &self.secrets
    }
    /// Returns resource requirements.
    #[must_use]
    pub const fn resources(&self) -> &ResourceLimits {
        &self.resources
    }
    /// Returns terminal requirements.
    #[must_use]
    pub const fn terminal(&self) -> TerminalRequirements {
        self.terminal
    }
}
