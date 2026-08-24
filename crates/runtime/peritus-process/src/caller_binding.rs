//! Optional dependency-neutral parent invocation binding.

use peritus_policy::ActorRole;
use peritus_types::{ActionId, ActorId, CapabilityName, EnvironmentId, ResourceId, Sha256Digest};

/// Exact authorized principal and target carried from the C4 router permit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionCallerTarget {
    actor_id: ActorId,
    role: ActorRole,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
}

impl ExecutionCallerTarget {
    /// Creates the principal/target portion of an execution caller binding.
    #[must_use]
    pub const fn new(
        actor_id: ActorId,
        role: ActorRole,
        environment_id: EnvironmentId,
        resource_id: ResourceId,
    ) -> Self {
        Self { actor_id, role, environment_id, resource_id }
    }

    /// Returns the authorized actor identity.
    #[must_use]
    pub const fn actor_id(self) -> ActorId {
        self.actor_id
    }

    /// Returns the authenticated actor role.
    #[must_use]
    pub const fn role(self) -> ActorRole {
        self.role
    }

    /// Returns the authorized execution environment.
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId {
        self.environment_id
    }

    /// Returns the authorized resource.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId {
        self.resource_id
    }
}

/// Exact higher-layer invocation identity carried in C2 plan canonical bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionCallerBinding {
    action_id: ActionId,
    capability_name: CapabilityName,
    descriptor_digest: Sha256Digest,
    prepared_digest: Sha256Digest,
    actor_id: ActorId,
    role: ActorRole,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
}

impl ExecutionCallerBinding {
    /// Creates an exact parent invocation binding without granting process authority.
    #[must_use]
    pub const fn new(
        action_id: ActionId,
        capability_name: CapabilityName,
        descriptor_digest: Sha256Digest,
        prepared_digest: Sha256Digest,
        target: ExecutionCallerTarget,
    ) -> Self {
        Self {
            action_id,
            capability_name,
            descriptor_digest,
            prepared_digest,
            actor_id: target.actor_id,
            role: target.role,
            environment_id: target.environment_id,
            resource_id: target.resource_id,
        }
    }

    /// Returns the parent tool action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the exact parent capability/tool name.
    #[must_use]
    pub const fn capability_name(&self) -> &CapabilityName {
        &self.capability_name
    }
    /// Returns the parent descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest {
        self.descriptor_digest
    }
    /// Returns the parent prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
    /// Returns the parent actor identity.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the parent authenticated actor role.
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    /// Returns the parent execution environment identity.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the parent authorized resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
}
