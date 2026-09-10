//! Effect-free target identity for a separately authenticated registered-folder patch.

use peritus_types::{
    ActionId, ActorId, EnvironmentId, Generation, ResourceId, RevisionNumber, SessionId,
    WorkspaceId,
};

/// Exact nominal identities supplied by the authenticated host before planning a folder patch.
///
/// Construction is deliberately unprivileged. The G0 host remains responsible for establishing
/// that the actor and session are authenticated, the folder mapping is trusted and permitted,
/// and the exact init or rewind command was explicitly reviewed and confirmed. This request has
/// no boolean substitute for any of those checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FolderPatchAuthorityPlanRequest {
    pub(in crate::developer_tools) actor_id: ActorId,
    pub(in crate::developer_tools) session_id: SessionId,
    pub(in crate::developer_tools) action_id: ActionId,
    pub(in crate::developer_tools) workspace_id: WorkspaceId,
    pub(in crate::developer_tools) resource_id: ResourceId,
    pub(in crate::developer_tools) environment_id: EnvironmentId,
    pub(in crate::developer_tools) generation: Generation,
    pub(in crate::developer_tools) revision: RevisionNumber,
}

impl FolderPatchAuthorityPlanRequest {
    /// Binds the exact authenticated caller, registered target, and current workspace fence.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "every independently authenticated identity remains explicit at the G0 boundary"
    )]
    pub const fn new(
        actor_id: ActorId,
        session_id: SessionId,
        action_id: ActionId,
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Self {
        Self {
            actor_id,
            session_id,
            action_id,
            workspace_id,
            resource_id,
            environment_id,
            generation,
            revision,
        }
    }
}
