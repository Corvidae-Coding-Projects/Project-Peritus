//! Authorized atomic patch application.

mod recovery;

use peritus_patch::{AppliedPatch, PatchIdentity, PatchPlan, PatchSet, RollbackStatus};
use peritus_types::{ActionId, Generation, ResourceId, RevisionNumber, WorkspaceId};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceAuthorizationRequest, WorkspaceCondition, WorkspaceError,
    WorkspaceGateway, WorkspaceOperation,
    consumption::{self, ActionConsumptionBinding, ActionTerminalRecord},
    gateway::{AuthorizationTarget, MutationPermit, validate_authority},
};

/// Durable classification of a previously authorized patch effect.
pub enum MutationRecoveryOutcome {
    /// The action was consumed but no patch transaction was ever published.
    NotAttempted,
    /// The patch was installed and its exact installed manifest was retained.
    AlreadyApplied(MutationOutcome),
    /// Recovery verified that every preimage is present.
    RolledBack,
    /// Current targets disagree with both the exact preimage and postimage.
    Dirty,
    /// Recovery could not prove a safe terminal outcome.
    Indeterminate,
}

/// Owned one-use patch authorization that can cross a cancellable worker boundary.
///
/// The authority references are validated and the action marker is durably consumed before this
/// value is returned. It contains only the checked plan and exact target binding needed to finish
/// that operation; callers cannot construct or clone it.
pub struct AuthorizedPatch {
    permit: MutationPermit,
    binding: ActionConsumptionBinding,
    plan: PatchPlan,
}

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
        let prepared = self.prepare_patch(authorization, patch)?;
        self.apply_prepared_patch(prepared)
    }

    /// Validates and durably consumes one exact patch action, returning an owned operation token.
    ///
    /// The token is suitable for a cancellable worker because it contains no borrowed authority
    /// records. Dropping it before `apply_prepared_patch` leaves a consumed action with no
    /// transaction; recovery can classify that state as not attempted after proving the exact
    /// transaction path is absent.
    ///
    /// # Errors
    ///
    /// Returns before returning a token when authority, workspace binding, or patch planning
    /// fails. Once returned, the action is durably consumed and must be completed or recovered.
    pub fn prepare_patch(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        patch: PatchSet,
    ) -> Result<AuthorizedPatch, WorkspaceError> {
        let payload = patch_payload(&patch, authorization.caller_binding());
        let state = self.state();
        let plan = patch
            .plan(state.binding().workspace_id(), state.generation(), state.revision())
            .map_err(|_| patch_error("patch does not match current workspace state"))?;
        let permit = self.authorize(authorization, &payload)?;
        if plan.expected_generation() != permit.generation()
            || plan.expected_revision() != permit.revision()
        {
            return Err(patch_error("planned patch differs from the one-use permit"));
        }
        Ok(AuthorizedPatch {
            permit,
            binding: ActionConsumptionBinding::from_state(self.state()),
            plan,
        })
    }

    /// Applies a previously authorized owned patch operation without borrowed authority records.
    ///
    /// # Errors
    ///
    /// Rejects a token whose exact target or consumed action no longer matches this gateway.
    /// Patch failures that cannot prove rollback set the workspace to indeterminate and require
    /// restart reconciliation.
    pub fn apply_prepared_patch(
        &mut self,
        prepared: AuthorizedPatch,
    ) -> Result<MutationOutcome, WorkspaceError> {
        self.apply_prepared_patch_cancellable(prepared, || false)?
            .ok_or_else(|| patch_error("patch cancellation was reported without a request"))
    }

    /// Applies a previously authorized patch and observes cancellation only before the first
    /// workspace target mutation. `Ok(None)` means the action was durably recorded as rolled
    /// back with the workspace unchanged; after mutation begins, cancellation is deferred until
    /// the transaction has a verified terminal result.
    ///
    /// # Errors
    ///
    /// Rejects stale prepared authority and returns patch or durable-receipt failures. An
    /// indeterminate result fences the workspace for reconciliation.
    pub fn apply_prepared_patch_cancellable(
        &mut self,
        prepared: AuthorizedPatch,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<MutationOutcome>, WorkspaceError> {
        let AuthorizedPatch { permit, binding, plan } = prepared;
        if !self.prepared_action_matches(binding, &permit, &plan) {
            return Err(patch_error("prepared patch no longer matches its consumed action"));
        }
        let root = self.workspace_mut().root().to_owned();
        let transaction_root = self.workspace_mut().transaction_root().to_owned();
        let action_id = permit.action_id();
        let action_digest = permit.action_digest();
        let result = peritus_patch::apply_patch_with_completion_and_cancellation(
            root,
            transaction_root.clone(),
            &plan,
            cancelled,
            |applied| {
                let terminal = applied.map_or(ActionTerminalRecord::RolledBack, |applied| {
                    ActionTerminalRecord::Applied {
                        patch_identity: applied.identity(),
                        installed_manifest: applied.installed_manifest().to_vec(),
                    }
                });
                consumption::complete_action(
                    &transaction_root,
                    binding,
                    action_id,
                    action_digest,
                    &terminal,
                )
                .map_err(|_| receipt_persistence_error())
            },
        );
        let applied = match result {
            Ok(Some(applied)) => applied,
            Ok(None) => {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Clean);
                return Ok(None);
            }
            Err(error) => {
                let transaction_exists = self
                    .workspace_mut()
                    .transaction_root()
                    .join(format!("txn-{}", plan.identity().to_hex()))
                    .try_exists()
                    .unwrap_or(true);
                let condition = if error.rollback_status() == RollbackStatus::Indeterminate
                    || transaction_exists
                {
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
        Ok(Some(MutationOutcome {
            action_id,
            workspace_id: self.state().binding().workspace_id(),
            resource_id: self.state().binding().resource_id(),
            generation: permit.generation(),
            revision: permit.revision(),
            patch: applied,
        }))
    }

    /// Cancels a prepared operation only when its exact patch transaction is absent.
    ///
    /// This is for cancellation observed before `apply_prepared_patch` begins. An existing or
    /// uninspectable transaction is retained as indeterminate evidence and fences the workspace.
    ///
    /// # Errors
    ///
    /// Rejects a stale token or any state where the exact transaction path cannot be proven absent.
    pub fn cancel_prepared_patch(
        &mut self,
        prepared: AuthorizedPatch,
    ) -> Result<(), WorkspaceError> {
        let AuthorizedPatch { permit, binding, plan } = prepared;
        if !self.prepared_action_matches(binding, &permit, &plan) {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
            return Err(recovery_error(
                "prepared patch cancellation lost its exact action binding",
            ));
        }
        let transaction = self
            .workspace_mut()
            .transaction_namespace()
            .join(format!("txn-{}", plan.identity().to_hex()));
        match std::fs::symlink_metadata(&transaction) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let transaction_root = self.workspace_mut().transaction_root().to_owned();
                if consumption::complete_action(
                    &transaction_root,
                    binding,
                    permit.action_id(),
                    permit.action_digest(),
                    &ActionTerminalRecord::RolledBack,
                )
                .is_err()
                {
                    self.workspace_mut()
                        .state_mut()
                        .set_condition(WorkspaceCondition::Indeterminate);
                    return Err(recovery_error(
                        "prepared patch cancellation could not persist its terminal receipt",
                    ));
                }
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Clean);
                Ok(())
            }
            Ok(_) | Err(_) => {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
                Err(recovery_error(
                    "prepared patch cancellation could not prove its transaction absent",
                ))
            }
        }
    }

    fn prepared_action_matches(
        &self,
        binding: ActionConsumptionBinding,
        permit: &MutationPermit,
        plan: &PatchPlan,
    ) -> bool {
        self.state().condition() == WorkspaceCondition::Clean
            && binding == ActionConsumptionBinding::from_state(self.state())
            && self.state().consumed_action_digest(permit.action_id())
                == Some(permit.action_digest())
            && plan.expected_generation() == permit.generation()
            && plan.expected_revision() == permit.revision()
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

const fn recovery_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Reconcile,
        RecoveryClass::Reconcile,
        detail,
    )
}

const fn receipt_persistence_error() -> peritus_patch::PatchError {
    peritus_patch::PatchError::completion_persistence_failure()
}
