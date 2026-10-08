//! Authorized atomic patch application.

use peritus_patch::{AppliedPatch, PatchIdentity, PatchSet, RollbackStatus};
use peritus_types::{ActionId, Generation, ResourceId, RevisionNumber, WorkspaceId};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceAuthorizationRequest, WorkspaceCondition, WorkspaceError,
    WorkspaceGateway, WorkspaceOperation,
    mutation_record::{
        MutationOperationReference, MutationOutcomeReference, persist_operation, persist_outcome,
    },
};

/// Successful filesystem transaction observation. Candidate creation remains a separate,
/// independently authorized operation before the workspace is clean again.
pub struct MutationOutcome {
    reference: MutationOutcomeReference,
    patch: AppliedPatch,
}

impl MutationOutcome {
    /// Returns the exact authorized action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.reference.action_id()
    }
    /// Returns the exact workspace lineage whose filesystem was changed.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.reference.workspace_id()
    }
    /// Returns the exact authorized resource whose filesystem was changed.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.reference.resource_id()
    }
    /// Returns the generation in which the patch was installed.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.reference.generation()
    }
    /// Returns the unchanged logical revision pending candidate creation.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.reference.revision()
    }
    /// Returns the applied canonical patch identity.
    #[must_use]
    pub const fn patch_identity(&self) -> PatchIdentity {
        self.reference.patch_identity()
    }
    /// Borrows exact durable patch-transaction evidence.
    #[must_use]
    pub const fn applied_patch(&self) -> &AppliedPatch {
        &self.patch
    }

    /// Returns the durable restart-visible handoff to candidate creation.
    #[must_use]
    pub const fn reference(&self) -> MutationOutcomeReference {
        self.reference
    }
}

/// Authorized durable patch operation which has not crossed the filesystem effect boundary.
pub struct PreparedWorkspaceMutation {
    operation: MutationOperationReference,
    plan: peritus_patch::PatchPlan,
}

impl PreparedWorkspaceMutation {
    /// Returns the durable operation identity retained before mutation begins.
    #[must_use]
    pub const fn operation_reference(&self) -> MutationOperationReference { self.operation }
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
        let prepared = self.prepare_patch(authorization, patch)?;
        self.apply_prepared_patch(prepared)
    }

    /// Consumes authority and durably records an exact operation before any patch effect.
    ///
    /// # Errors
    /// Returns on authority, planning, binding, or durable-record failure before target mutation.
    pub fn prepare_patch(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        patch: PatchSet,
    ) -> Result<PreparedWorkspaceMutation, WorkspaceError> {
        let payload = patch_payload(&patch, authorization.caller_binding());
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
        let operation = MutationOperationReference::new(
            &permit,
            state.binding().workspace_id(),
            state.binding().resource_id(),
            plan.identity(),
        );
        persist_operation(self.transaction_namespace(), operation)?;
        Ok(PreparedWorkspaceMutation { operation, plan })
    }

    /// Applies one already authorized and durably retained operation.
    ///
    /// # Errors
    /// Preserves typed patch recovery and fences indeterminate or unrecorded success.
    pub fn apply_prepared_patch(
        &mut self,
        prepared: PreparedWorkspaceMutation,
    ) -> Result<MutationOutcome, WorkspaceError> {
        let operation = prepared.operation;
        if self.state().condition() != WorkspaceCondition::Clean
            || operation.workspace_id() != self.state().binding().workspace_id()
            || operation.resource_id() != self.state().binding().resource_id()
            || operation.generation() != self.state().generation()
            || operation.revision() != self.state().revision()
            || operation.patch_identity() != prepared.plan.identity()
        {
            return Err(WorkspaceError::new(
                ErrorCode::StaleWorkspace,
                WorkspaceOperation::Mutate,
                RecoveryClass::Reconcile,
                "prepared mutation differs from current workspace ownership",
            ));
        }
        let root = self.workspace_mut().root().to_owned();
        let transaction_root = self.workspace_mut().transaction_root().to_owned();
        let result = peritus_patch::apply_patch(root, transaction_root, &prepared.plan);
        let applied = match result {
            Ok(applied) => applied,
            Err(error) => {
                let condition = if error.rollback_status() == RollbackStatus::Indeterminate {
                    WorkspaceCondition::Indeterminate
                } else {
                    WorkspaceCondition::Clean
                };
                self.workspace_mut().state_mut().set_condition(condition);
                return Err(WorkspaceError::from_patch(&error, condition));
            }
        };
        let reference = MutationOutcomeReference::new(operation, applied.manifest_digest());
        if let Err(error) = persist_outcome(self.transaction_namespace(), reference) {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
            return Err(error);
        }
        self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Dirty);
        Ok(MutationOutcome { reference, patch: applied })
    }
}

/// Returns canonical adapter payload bytes to bind into [`peritus_protocol::ActionIntentDto`].
#[must_use]
pub fn patch_authorization_payload(patch: &PatchSet) -> Vec<u8> {
    patch_payload(patch, None)
}

/// Returns canonical patch payload bytes bound to one exact validated C4 caller.
#[must_use]
pub fn patch_authorization_payload_for_caller(
    patch: &PatchSet,
    caller: &crate::WorkspaceCallerBinding,
) -> Vec<u8> {
    patch_payload(patch, Some(caller))
}

fn patch_payload(patch: &PatchSet, caller: Option<&crate::WorkspaceCallerBinding>) -> Vec<u8> {
    let mut bytes = if caller.is_some() {
        b"PERITUS-WORKSPACE-PATCH-V2\0".to_vec()
    } else {
        b"PERITUS-WORKSPACE-PATCH-V1\0".to_vec()
    };
    bytes.extend_from_slice(patch.workspace_id().as_bytes());
    bytes.extend_from_slice(&patch.expected_generation().get().to_be_bytes());
    bytes.extend_from_slice(&patch.expected_revision().get().to_be_bytes());
    bytes.extend_from_slice(patch.identity().as_bytes());
    if let Some(caller) = caller {
        crate::caller::append_caller(&mut bytes, Some(caller));
    }
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
