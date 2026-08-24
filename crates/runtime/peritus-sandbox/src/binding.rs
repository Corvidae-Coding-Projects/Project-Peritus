//! Exact target identity bound into every sandbox plan.

use peritus_types::{EnvironmentId, ProcessId, ResourceId, RevisionTuple};

/// Exact process, resource, environment, and revision target for one sandbox plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SandboxBinding {
    process_id: ProcessId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    revision: RevisionTuple,
}

impl SandboxBinding {
    /// Creates one exact inert sandbox target binding.
    #[must_use]
    pub const fn new(
        process_id: ProcessId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        revision: RevisionTuple,
    ) -> Self {
        Self { process_id, resource_id, environment_id, revision }
    }

    /// Returns the durable process identity.
    #[must_use]
    pub const fn process_id(self) -> ProcessId {
        self.process_id
    }
    /// Returns the exact capability-addressable target.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId {
        self.resource_id
    }
    /// Returns the isolated execution environment.
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the complete immutable revision identity.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
}
