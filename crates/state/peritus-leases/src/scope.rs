//! Exact lease scope, holder identity, and checked duration values.

use crate::LeaseError;
use peritus_types::{ActorId, EnvironmentId, ResourceId, SessionId, WorkspaceId};
use vstd::prelude::*;

verus! {

/// The exact resource identity protected by one workspace-lineage aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseScope {
    pub(crate) workspace: WorkspaceId,
    pub(crate) resource: ResourceId,
    pub(crate) environment: EnvironmentId,
}

impl LeaseScope {
    /// Returns the exact workspace identity used by specifications.
    pub closed spec fn spec_workspace_id(&self) -> WorkspaceId { self.workspace }

    /// Returns the exact resource identity used by specifications.
    pub closed spec fn spec_resource_id(&self) -> ResourceId { self.resource }

    /// Returns the exact environment identity used by specifications.
    pub closed spec fn spec_environment_id(&self) -> EnvironmentId { self.environment }

    /// Creates an exact lease scope from validated nominal identifiers.
    #[must_use]
    pub const fn new(
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
    ) -> (scope: Self)
        ensures
            scope.spec_workspace_id() == workspace_id,
            scope.spec_resource_id() == resource_id,
            scope.spec_environment_id() == environment_id,
    {
        Self { workspace: workspace_id, resource: resource_id, environment: environment_id }
    }

    /// Returns the aggregate key and workspace lineage.
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId { self.workspace }

    /// Returns the exact resolved resource identity.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId { self.resource }

    /// Returns the exact execution environment identity.
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId { self.environment }
}

/// The exact actor and session that hold an active lease.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseHolder {
    pub(crate) actor_id: ActorId,
    pub(crate) session_id: SessionId,
}

impl LeaseHolder {
    /// Returns the exact actor identity used by specifications.
    pub closed spec fn spec_actor_id(&self) -> ActorId { self.actor_id }

    /// Returns the exact session identity used by specifications.
    pub closed spec fn spec_session_id(&self) -> SessionId { self.session_id }

    /// Creates a holder identity from validated nominal identifiers.
    #[must_use]
    pub const fn new(actor_id: ActorId, session_id: SessionId) -> (holder: Self)
        ensures
            holder.spec_actor_id() == actor_id,
            holder.spec_session_id() == session_id,
    {
        Self { actor_id, session_id }
    }

    /// Returns the exact actor.
    #[must_use]
    pub const fn actor_id(self) -> ActorId { self.actor_id }

    /// Returns the exact session.
    #[must_use]
    pub const fn session_id(self) -> SessionId { self.session_id }
}

/// A strictly positive lease duration in authority-clock milliseconds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseDuration {
    pub(crate) millis: u64,
}

impl LeaseDuration {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_millis() > 0 }

    /// Returns the exact mathematical millisecond duration used by specifications.
    pub closed spec fn spec_millis(&self) -> int { self.millis as int }

    /// Creates a nonzero duration.
    ///
    /// # Errors
    ///
    /// Returns [`LeaseError::ZeroDuration`] when `millis` is zero.
    pub const fn new(millis: u64) -> (result: Result<Self, LeaseError>)
        ensures
            match result {
                Ok(duration) => millis > 0 && duration.spec_millis() == millis as int,
                Err(error) => millis == 0 && error == LeaseError::ZeroDuration,
            },
    {
        if millis == 0 { Err(LeaseError::ZeroDuration) } else { Ok(Self { millis }) }
    }

    /// Returns the exact millisecond duration.
    #[must_use]
    pub const fn millis(self) -> (result: u64)
        ensures result as int == self.spec_millis(),
    { self.millis }
}

} // verus!
