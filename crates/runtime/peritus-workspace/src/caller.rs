//! Dependency-neutral C4 caller and immutable target bindings.

use peritus_policy::ActorRole;
use peritus_types::{
    ActionId, ActorId, CapabilityName, EnvironmentId, ResourceId, Sha256Digest, WorkspaceId,
};

/// Exact validated C4 caller facts bound into one C1 authorization payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceCallerBinding {
    action_id: ActionId,
    actor_id: ActorId,
    role: ActorRole,
    workspace_id: WorkspaceId,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
    capability_name: CapabilityName,
    descriptor_digest: Sha256Digest,
    prepared_digest: Sha256Digest,
}

impl WorkspaceCallerBinding {
    /// Creates an exact dependency-neutral projection of a validated C4 invocation permit.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        action_id: ActionId,
        actor_id: ActorId,
        role: ActorRole,
        workspace_id: WorkspaceId,
        environment_id: EnvironmentId,
        resource_id: ResourceId,
        capability_name: CapabilityName,
        descriptor_digest: Sha256Digest,
        prepared_digest: Sha256Digest,
    ) -> Self {
        Self {
            action_id,
            actor_id,
            role,
            workspace_id,
            environment_id,
            resource_id,
            capability_name,
            descriptor_digest,
            prepared_digest,
        }
    }

    /// Returns the exact tool action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the authenticated actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the authenticated actor role.
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    /// Returns the exact workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the exact environment.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the exact target resource.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
    /// Borrows the exact capability/tool name.
    #[must_use]
    pub const fn capability_name(&self) -> &CapabilityName {
        &self.capability_name
    }
    /// Returns the exact descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest {
        self.descriptor_digest
    }
    /// Returns the exact prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
}

/// Exact resource/environment identity attached to an immutable workspace handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "the binding distinguishes three independent nominal identity domains"
)]
pub struct ReadOnlyTargetBinding {
    workspace_id: WorkspaceId,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
}

impl ReadOnlyTargetBinding {
    pub(crate) const fn new(
        workspace_id: WorkspaceId,
        environment_id: EnvironmentId,
        resource_id: ResourceId,
    ) -> Self {
        Self { workspace_id, environment_id, resource_id }
    }
    /// Returns the workspace lineage.
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the environment identity.
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the resource identity.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId {
        self.resource_id
    }
}

pub fn append_caller(bytes: &mut Vec<u8>, caller: Option<&WorkspaceCallerBinding>) {
    let Some(caller) = caller else {
        bytes.push(0);
        return;
    };
    bytes.push(1);
    bytes.extend_from_slice(caller.action_id.as_bytes());
    bytes.extend_from_slice(caller.actor_id.as_bytes());
    bytes.push(role_tag(caller.role));
    bytes.extend_from_slice(caller.workspace_id.as_bytes());
    bytes.extend_from_slice(caller.environment_id.as_bytes());
    bytes.extend_from_slice(caller.resource_id.as_bytes());
    put_bytes(bytes, caller.capability_name.as_str().as_bytes());
    bytes.extend_from_slice(caller.descriptor_digest.as_bytes());
    bytes.extend_from_slice(caller.prepared_digest.as_bytes());
}

fn put_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value);
}

const fn role_tag(role: ActorRole) -> u8 {
    match role {
        ActorRole::Writer => 1,
        ActorRole::Fixer => 2,
        ActorRole::Reviewer => 3,
        ActorRole::Evaluator => 4,
        ActorRole::GateRunner => 5,
        ActorRole::Orchestrator => 6,
        ActorRole::EvolutionAgent => 7,
        ActorRole::HumanAuthority => 8,
        ActorRole::DaemonService => 9,
        ActorRole::ProviderToolWorker => 10,
        ActorRole::Plugin => 11,
    }
}
