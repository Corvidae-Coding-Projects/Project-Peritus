//! Exact-authority patch application for one registered ordinary folder.

use std::{collections::BTreeMap, path::PathBuf};

use peritus_leases::LeaseHolder;
use peritus_patch::{AppliedPatch, PatchSet, RollbackStatus};
use peritus_types::{
    ActionId, EnvironmentId, Generation, ResourceId, RevisionNumber, RevisionTuple, Sha256Digest,
    WorkspaceId,
};

use crate::{
    ErrorCode, FolderIdentity, RecoveryClass, WorkspaceAuthorizationRequest, WorkspaceError,
    WorkspaceOperation,
    consumption::{self, ActionConsumptionBinding},
    gateway::{AuthorizationTarget, validate_authority},
};

mod binding;
use binding::{action_binding, folder_patch_payload, revalidate_identity};
mod recovery;
pub use binding::folder_patch_action_intent;
pub use recovery::{
    FolderMutationActionMarker, FolderMutationRecoveryOutcome, FolderMutationRecoveryRequest,
    FolderMutationRecoveryState, recover_folder_mutation,
};

/// Unprivileged inputs for opening one exact registered-folder mutation owner.
pub struct FolderMutationOpenRequest {
    identity: FolderIdentity,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    revision: RevisionTuple,
    lease_holder: LeaseHolder,
    transaction_root: PathBuf,
}

/// Closed readiness state for one ordinary-folder mutation owner.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FolderMutationCondition {
    /// No unresolved transaction is known; exact authority is still required for every effect.
    Ready,
    /// Mutation or cleanup could not prove a terminal state; explicit recovery is required.
    Indeterminate,
}

impl FolderMutationOpenRequest {
    /// Binds a registered folder to its nominal target, current revision, holder, and transaction
    /// storage. [`FolderMutationGateway::open`] revalidates every supplied fact it can observe.
    #[must_use]
    pub fn new(
        identity: FolderIdentity,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        revision: RevisionTuple,
        lease_holder: LeaseHolder,
        transaction_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            identity,
            resource_id,
            environment_id,
            revision,
            lease_holder,
            transaction_root: transaction_root.into(),
        }
    }
}

/// Sole move-only mutation owner for one exact non-Git registered folder.
pub struct FolderMutationGateway {
    identity: FolderIdentity,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    revision: RevisionTuple,
    lease_holder: LeaseHolder,
    transaction_root: PathBuf,
    consumed_actions: BTreeMap<ActionId, Sha256Digest>,
    condition: FolderMutationCondition,
}

impl FolderMutationGateway {
    /// Opens one exact ordinary-folder owner without fabricating Git state.
    ///
    /// # Errors
    ///
    /// Rejects changed folder identity, overlapping transaction storage, corrupt replay markers,
    /// or any restart-visible patch transaction whose terminal state is not already proven.
    pub fn open(request: FolderMutationOpenRequest) -> Result<Self, WorkspaceError> {
        let FolderMutationOpenRequest {
            identity,
            resource_id,
            environment_id,
            revision,
            lease_holder,
            transaction_root,
        } = request;
        revalidate_identity(&identity, WorkspaceOperation::Open)?;
        let transaction_root = crate::transaction_namespace::open_folder(
            transaction_root,
            revision.workspace_id(),
            resource_id,
            environment_id,
            identity.digest(),
            identity.root(),
        )?;
        if !crate::transaction_namespace::folder_is_terminal(&transaction_root)? {
            return Err(unresolved("registered folder has an unresolved patch transaction"));
        }
        let binding = action_binding(revision, resource_id, environment_id);
        let consumed_actions = consumption::restore(&transaction_root, binding)?;
        Ok(Self {
            identity,
            resource_id,
            environment_id,
            revision,
            lease_holder,
            transaction_root,
            consumed_actions,
            condition: FolderMutationCondition::Ready,
        })
    }

    /// Borrows the exact registered folder identity.
    #[must_use]
    pub const fn identity(&self) -> &FolderIdentity {
        &self.identity
    }

    /// Returns the bound workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.revision.workspace_id()
    }

    /// Returns the exact authorized resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Returns the current fenced generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.revision.workspace_generation()
    }

    /// Returns the current logical revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision.workspace_revision()
    }

    /// Returns whether further mutations are currently safe to authorize.
    #[must_use]
    pub const fn condition(&self) -> FolderMutationCondition {
        self.condition
    }

    /// Returns the isolated target-owned transaction namespace for diagnostics.
    #[must_use]
    pub fn transaction_namespace(&self) -> &std::path::Path {
        &self.transaction_root
    }

    /// Builds the exact folder-and-patch payload that C0 must commit for this mutation.
    ///
    /// # Errors
    ///
    /// Rejects an unavailable owner, changed folder identity, or a patch for another revision.
    pub fn authorization_payload(&self, patch: &PatchSet) -> Result<Vec<u8>, WorkspaceError> {
        self.require_clean()?;
        revalidate_identity(&self.identity, WorkspaceOperation::Authorize)?;
        self.validate_patch_binding(patch)?;
        Ok(folder_patch_payload(self, patch))
    }

    /// Validates exact committed authority and atomically applies one checked patch.
    ///
    /// The action is durably consumed before the first patch effect. A preimage conflict leaves
    /// target bytes untouched. Indeterminate rollback or pending cleanup fences later mutation.
    ///
    /// # Errors
    ///
    /// Returns before effect on target, authority, identity, or preimage mismatch. Failures that
    /// cannot prove terminal transaction state fence this owner for explicit reconciliation.
    pub fn apply_patch(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        patch: PatchSet,
    ) -> Result<FolderMutationOutcome, WorkspaceError> {
        self.require_clean()?;
        if authorization.revision() != self.revision {
            return Err(WorkspaceError::new(
                ErrorCode::AuthorizationMismatch,
                WorkspaceOperation::Authorize,
                RecoveryClass::CorrectRequest,
                "committed authority differs from the registered folder revision tuple",
            ));
        }
        let action_id = authorization.intent().action_id;
        consumption::contains_action(&self.consumed_actions, action_id)?;
        let payload = self.authorization_payload(&patch)?;
        let expected_intent = folder_patch_action_intent(
            self.lease_holder.actor_id(),
            action_id,
            self.environment_id,
            self.resource_id,
            payload.clone(),
        )?;
        if authorization.intent() != &expected_intent {
            return Err(WorkspaceError::new(
                ErrorCode::AuthorizationMismatch,
                WorkspaceOperation::Authorize,
                RecoveryClass::Reauthorize,
                "committed action differs from the canonical recoverable folder-patch intent",
            ));
        }
        let permit = validate_authority(self.authorization_target(), authorization, &payload)?;
        let binding = self.action_binding();
        if let Err(error) = consumption::commit(
            &self.transaction_root,
            binding,
            self.consumed_actions.len(),
            action_id,
            permit.action_digest(),
        ) {
            if error.code() == ErrorCode::Indeterminate {
                self.condition = FolderMutationCondition::Indeterminate;
            }
            return Err(error);
        }
        self.consumed_actions.insert(action_id, permit.action_digest());
        let plan = patch
            .plan(self.workspace_id(), self.generation(), self.revision())
            .map_err(|_| patch_error(RecoveryClass::CorrectRequest, "patch version is stale"))?;
        if plan.expected_generation() != permit.generation()
            || plan.expected_revision() != permit.revision()
        {
            return Err(patch_error(
                RecoveryClass::CorrectRequest,
                "planned patch differs from the one-use permit",
            ));
        }
        if let Err(error) = revalidate_identity(&self.identity, WorkspaceOperation::Mutate) {
            self.condition = FolderMutationCondition::Indeterminate;
            return Err(error);
        }
        let result =
            peritus_patch::apply_patch(self.identity.root(), &self.transaction_root, &plan);
        match result {
            Ok(applied) => {
                let terminal =
                    crate::transaction_namespace::folder_is_terminal(&self.transaction_root)
                        .unwrap_or(false);
                self.condition = if !applied.cleanup_pending() && terminal {
                    FolderMutationCondition::Ready
                } else {
                    FolderMutationCondition::Indeterminate
                };
                Ok(FolderMutationOutcome {
                    action_id: permit.action_id(),
                    workspace_id: self.workspace_id(),
                    resource_id: self.resource_id,
                    generation: permit.generation(),
                    revision: permit.revision(),
                    condition: self.condition,
                    patch: applied,
                })
            }
            Err(error) => {
                let terminal =
                    crate::transaction_namespace::folder_is_terminal(&self.transaction_root)
                        .unwrap_or(false);
                let indeterminate =
                    error.rollback_status() == RollbackStatus::Indeterminate || !terminal;
                self.condition = if indeterminate {
                    FolderMutationCondition::Indeterminate
                } else {
                    FolderMutationCondition::Ready
                };
                Err(patch_error(
                    if indeterminate { RecoveryClass::Reconcile } else { RecoveryClass::Reobserve },
                    if indeterminate {
                        "checked folder patch failed without proven terminal recovery state"
                    } else {
                        "checked folder patch failed without changing target bytes"
                    },
                ))
            }
        }
    }

    fn require_clean(&self) -> Result<(), WorkspaceError> {
        if self.condition == FolderMutationCondition::Ready {
            Ok(())
        } else {
            Err(WorkspaceError::new(
                ErrorCode::WorkspaceUnavailable,
                WorkspaceOperation::Authorize,
                RecoveryClass::Reconcile,
                "registered folder is fenced pending transaction reconciliation",
            ))
        }
    }

    fn validate_patch_binding(&self, patch: &PatchSet) -> Result<(), WorkspaceError> {
        let exact = (patch.workspace_id(), patch.expected_generation(), patch.expected_revision())
            == (self.workspace_id(), self.generation(), self.revision());
        if !exact {
            return Err(patch_error(
                RecoveryClass::CorrectRequest,
                "patch does not match the registered folder revision",
            ));
        }
        Ok(())
    }

    const fn authorization_target(&self) -> AuthorizationTarget {
        AuthorizationTarget::new(
            self.workspace_id(),
            self.resource_id,
            self.environment_id,
            self.generation(),
            self.revision(),
            self.lease_holder,
        )
    }

    const fn action_binding(&self) -> ActionConsumptionBinding {
        action_binding(self.revision, self.resource_id, self.environment_id)
    }
}

/// Successful exact ordinary-folder patch evidence.
pub struct FolderMutationOutcome {
    action_id: ActionId,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: Generation,
    revision: RevisionNumber,
    condition: FolderMutationCondition,
    patch: AppliedPatch,
}

mod outcome;

const fn patch_error(recovery: RecoveryClass, detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(ErrorCode::Patch, WorkspaceOperation::Mutate, recovery, detail)
}

const fn unresolved(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Open,
        RecoveryClass::Reconcile,
        detail,
    )
}
