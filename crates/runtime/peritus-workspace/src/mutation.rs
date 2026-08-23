//! Authorized atomic patch application.

use peritus_patch::{AppliedPatch, PatchIdentity, PatchSet, RollbackStatus};
use peritus_types::{ActionId, Generation, ResourceId, RevisionNumber, WorkspaceId};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceAuthorizationRequest, WorkspaceCondition, WorkspaceError,
    WorkspaceGateway, WorkspaceOperation,
};

/// Successful filesystem transaction observation. Candidate creation remains a separate,
/// independently authorized operation before the workspace is clean again.
pub struct MutationOutcome {
    action_id: ActionId,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: Generation,
    revision: RevisionNumber,
    patch: AppliedPatch,
}

impl MutationOutcome {
    /// Returns the exact authorized action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the exact workspace lineage whose filesystem was changed.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the exact authorized resource whose filesystem was changed.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
    /// Returns the generation in which the patch was installed.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    /// Returns the unchanged logical revision pending candidate creation.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    /// Returns the applied canonical patch identity.
    #[must_use]
    pub const fn patch_identity(&self) -> PatchIdentity {
        self.patch.identity()
    }
    /// Borrows exact durable patch-transaction evidence.
    #[must_use]
    pub const fn applied_patch(&self) -> &AppliedPatch {
        &self.patch
    }
}

impl WorkspaceGateway {
    /// Authorizes and atomically applies one checked multi-file patch.
    ///
    /// The workspace becomes [`WorkspaceCondition::Dirty`] after success until a separately
    /// authorized candidate snapshots the exact observed result. This prevents the pre-patch
    /// durable snapshot from being reported as current.
    ///
    /// # Errors
    ///
    /// Returns before effect on authority or planning mismatch. Patch failures that cannot prove
    /// rollback set the workspace to indeterminate and require restart reconciliation.
    pub fn apply_patch(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        patch: PatchSet,
    ) -> Result<MutationOutcome, WorkspaceError> {
        let payload = patch_authorization_payload(&patch);
        let permit = self.authorize(authorization, &payload)?;
        let state = self.state();
        let plan = patch
            .plan(state.binding().workspace_id(), state.generation(), state.revision())
            .map_err(|_| patch_error("patch does not match current workspace state"))?;
        if plan.expected_generation() != permit.generation()
            || plan.expected_revision() != permit.revision()
        {
            return Err(patch_error("planned patch differs from the one-use permit"));
        }
        let root = self.workspace_mut().root().to_owned();
        let transaction_root = self.workspace_mut().transaction_root().to_owned();
        let result = peritus_patch::apply_patch(root, transaction_root, &plan);
        let applied = match result {
            Ok(applied) => applied,
            Err(error) => {
                let condition = if error.rollback_status() == RollbackStatus::Indeterminate {
                    WorkspaceCondition::Indeterminate
                } else {
                    WorkspaceCondition::Clean
                };
                self.workspace_mut().state_mut().set_condition(condition);
                return Err(WorkspaceError::new(
                    ErrorCode::Patch,
                    WorkspaceOperation::Mutate,
                    if condition == WorkspaceCondition::Indeterminate {
                        RecoveryClass::Reconcile
                    } else {
                        RecoveryClass::Reobserve
                    },
                    "checked patch transaction failed",
                ));
            }
        };
        self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Dirty);
        Ok(MutationOutcome {
            action_id: permit.action_id(),
            workspace_id: self.state().binding().workspace_id(),
            resource_id: self.state().binding().resource_id(),
            generation: permit.generation(),
            revision: permit.revision(),
            patch: applied,
        })
    }
}

/// Returns canonical adapter payload bytes to bind into [`peritus_protocol::ActionIntentDto`].
#[must_use]
pub fn patch_authorization_payload(patch: &PatchSet) -> Vec<u8> {
    let mut bytes = b"PERITUS-WORKSPACE-PATCH-V1\0".to_vec();
    bytes.extend_from_slice(patch.workspace_id().as_bytes());
    bytes.extend_from_slice(&patch.expected_generation().get().to_be_bytes());
    bytes.extend_from_slice(&patch.expected_revision().get().to_be_bytes());
    bytes.extend_from_slice(patch.identity().as_bytes());
    bytes
}

const fn patch_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Patch,
        WorkspaceOperation::Mutate,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
