//! Complete owner lineage for one operating-system execution.

use peritus_types::{
    ActionId, ActorId, AttemptId, EnvironmentId, ProcessId, ProjectId, ResourceId, RevisionTuple,
    RunId, SessionId, TurnId, WorkspaceId,
};

/// Exact durable owner identity for one process and all descendants/support tasks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionIdentity {
    project_id: ProjectId,
    session_id: SessionId,
    run_id: RunId,
    attempt_id: AttemptId,
    turn_id: TurnId,
    action_id: ActionId,
    process_id: ProcessId,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    actor_id: ActorId,
    revision: RevisionTuple,
}

impl ExecutionIdentity {
    /// Creates a complete nominal process owner identity.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        project_id: ProjectId,
        session_id: SessionId,
        run_id: RunId,
        attempt_id: AttemptId,
        turn_id: TurnId,
        action_id: ActionId,
        process_id: ProcessId,
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        actor_id: ActorId,
        revision: RevisionTuple,
    ) -> Self {
        Self {
            project_id,
            session_id,
            run_id,
            attempt_id,
            turn_id,
            action_id,
            process_id,
            workspace_id,
            resource_id,
            environment_id,
            actor_id,
            revision,
        }
    }

    /// Returns the project owner.
    #[must_use]
    pub const fn project_id(self) -> ProjectId {
        self.project_id
    }
    /// Returns the session owner.
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }
    /// Returns the run owner.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    /// Returns the attempt owner.
    #[must_use]
    pub const fn attempt_id(self) -> AttemptId {
        self.attempt_id
    }
    /// Returns the turn owner.
    #[must_use]
    pub const fn turn_id(self) -> TurnId {
        self.turn_id
    }
    /// Returns the action owner.
    #[must_use]
    pub const fn action_id(self) -> ActionId {
        self.action_id
    }
    /// Returns the stable process record identity.
    #[must_use]
    pub const fn process_id(self) -> ProcessId {
        self.process_id
    }
    /// Returns the workspace identity.
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the exact resource identity.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId {
        self.resource_id
    }
    /// Returns the execution environment identity.
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the acting principal.
    #[must_use]
    pub const fn actor_id(self) -> ActorId {
        self.actor_id
    }
    /// Returns the complete authority revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
}
