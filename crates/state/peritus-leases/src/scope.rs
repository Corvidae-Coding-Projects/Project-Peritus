//! Exact lease scope, holder identity, and checked duration values.

use crate::LeaseError;
use peritus_types::{ActorId, EnvironmentId, ResourceId, SessionId, WorkspaceId};

#[doc = " The exact resource identity protected by one workspace-lineage aggregate."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseScope {
    pub(crate) workspace: WorkspaceId,
    pub(crate) resource: ResourceId,
    pub(crate) environment: EnvironmentId,
}
impl LeaseScope {
    #[doc = " Creates an exact lease scope from validated nominal identifiers."]
    #[must_use]
    pub const fn new(
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
    ) -> Self {
        Self { workspace: workspace_id, resource: resource_id, environment: environment_id }
    }
    #[doc = " Returns the aggregate key and workspace lineage."]
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId {
        self.workspace
    }
    #[doc = " Returns the exact resolved resource identity."]
    #[must_use]
    pub const fn resource_id(self) -> ResourceId {
        self.resource
    }
    #[doc = " Returns the exact execution environment identity."]
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId {
        self.environment
    }
}
#[doc = " The exact actor and session that hold an active lease."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseHolder {
    pub(crate) actor_id: ActorId,
    pub(crate) session_id: SessionId,
}
impl LeaseHolder {
    #[doc = " Creates a holder identity from validated nominal identifiers."]
    #[must_use]
    pub const fn new(actor_id: ActorId, session_id: SessionId) -> Self {
        Self { actor_id, session_id }
    }
    #[doc = " Returns the exact actor."]
    #[must_use]
    pub const fn actor_id(self) -> ActorId {
        self.actor_id
    }
    #[doc = " Returns the exact session."]
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }
}
#[doc = " A strictly positive lease duration in authority-clock milliseconds."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseDuration {
    pub(crate) millis: u64,
}
impl LeaseDuration {
    #[doc = " Creates a nonzero duration."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`LeaseError::ZeroDuration`] when `millis` is zero."]
    pub const fn new(millis: u64) -> Result<Self, LeaseError> {
        if millis == 0 { Err(LeaseError::ZeroDuration) } else { Ok(Self { millis }) }
    }
    #[doc = " Returns the exact millisecond duration."]
    #[must_use]
    pub const fn millis(self) -> u64 {
        self.millis
    }
}
