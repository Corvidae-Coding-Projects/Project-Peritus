//! Exact C0 authority for a separately authenticated registered-folder patch.

use std::sync::Arc;

use peritus_journal::{
    CommittedCapabilityUse, CommittedKernelTransition, CommittedLeaseTransition,
    CurrentAuthorityEpoch,
};
use peritus_leases::LeaseHolder;
use peritus_policy::{OperationClass, RiskClass};
use peritus_protocol::ActionIntentDto;
use peritus_spec::AcceptanceContract;
use peritus_types::{EnvironmentId, ResourceId, RevisionTuple, SessionId};
use peritus_workspace::WorkspaceAuthorizationRequest;

use super::{
    CommandRuntime, RuntimeInner, authority, contract,
    identity::{self, CommandIds},
    journal, kernel, lease,
};
use crate::{FolderPatchAuthorityPlanRequest, ProductRunnerError, ProductRunnerErrorKind};

const AUTHORITY_WINDOW_MILLIS: u64 = 60_000;

/// Inert, move-only target binding for one exact registered-folder patch authority commit.
///
/// Planning performs no target filesystem operation and confers no permission by itself.
pub struct FolderPatchAuthorityPlan {
    runtime: Arc<RuntimeInner>,
    contract: AcceptanceContract,
    ids: CommandIds,
}

impl FolderPatchAuthorityPlan {
    /// Returns the complete checked revision tuple required by the C1 mutation owner.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.ids.revision
    }

    /// Returns the exact actor/session lease holder required by the C1 mutation owner.
    #[must_use]
    pub const fn lease_holder(&self) -> LeaseHolder {
        LeaseHolder::new(self.ids.actor, self.ids.session)
    }

    /// Returns the registered resource identity required by the C1 mutation owner.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.ids.resource
    }

    /// Returns the registered environment identity required by the C1 mutation owner.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.ids.environment
    }
}

/// Owned committed receipts for one exact registered-folder patch.
///
/// The receipts authorize only the opaque payload committed by
/// [`CommandRuntime::commit_folder_patch_authority`]. C1 still validates and consumes them before
/// applying the exact patch.
pub struct FolderPatchAuthority {
    intent: ActionIntentDto,
    kernel: CommittedKernelTransition,
    capability: CommittedCapabilityUse,
    lease: CommittedLeaseTransition,
    epoch: CurrentAuthorityEpoch,
    revision: RevisionTuple,
    session_id: SessionId,
}

impl FolderPatchAuthority {
    /// Borrows a complete request from these owned receipts for the target-owned C1 gate.
    #[must_use]
    pub const fn request(&self) -> WorkspaceAuthorizationRequest<'_> {
        WorkspaceAuthorizationRequest::new(
            &self.intent,
            &self.kernel,
            &self.capability,
            &self.lease,
            &self.epoch,
            self.revision,
            self.session_id,
            self.revision.workspace_generation(),
            self.revision.workspace_revision(),
            authority::instant(20),
        )
    }
}

impl CommandRuntime {
    /// Plans exact C1 target bindings without committing authority or touching target bytes.
    ///
    /// The caller must first authenticate the exact actor, session, action, host trust and folder
    /// permission, and must verify explicit confirmation of the reviewed init or rewind command.
    /// This method does not perform or claim any of those G0 checks.
    ///
    /// # Errors
    /// Returns a typed error if the independent folder-authority ordinal or checked contract
    /// cannot be constructed.
    pub fn plan_folder_patch_authority(
        &self,
        request: FolderPatchAuthorityPlanRequest,
    ) -> Result<FolderPatchAuthorityPlan, ProductRunnerError> {
        let ordinal = {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| plan_error("folder-patch authority owner is poisoned"))?;
            state.next_folder_patch_ordinal = state
                .next_folder_patch_ordinal
                .checked_add(1)
                .ok_or_else(|| plan_error("folder-patch authority ordinal overflowed"))?;
            state.next_folder_patch_ordinal
        };
        let contract =
            contract::command_contract(self.inner.run_id, ordinal).map_err(plan_error)?;
        let ids = CommandIds::for_folder_patch(
            self.inner.run_id,
            ordinal,
            &contract,
            request.actor_id,
            request.session_id,
            request.action_id,
            request.workspace_id,
            request.resource_id,
            request.environment_id,
            request.generation,
            request.revision,
        )
        .map_err(plan_error)?;
        Ok(FolderPatchAuthorityPlan { runtime: Arc::clone(&self.inner), contract, ids })
    }

    /// Commits real C0 dispatch, capability, lease and current-epoch receipts for one payload.
    ///
    /// `payload` must be the opaque bytes returned by the already-open C1 folder mutation owner.
    /// This method records authority under the runtime state root but performs no target filesystem
    /// mutation; only C1 may apply the patch. G0's host trust, permission and reviewed-command
    /// authentication obligations remain mandatory and are not reinterpreted here.
    ///
    /// # Errors
    /// Rejects a plan created by another runtime instance or any failed checked commitment.
    pub fn commit_folder_patch_authority(
        &self,
        plan: FolderPatchAuthorityPlan,
        payload: Vec<u8>,
    ) -> Result<FolderPatchAuthority, ProductRunnerError> {
        if !Arc::ptr_eq(&self.inner, &plan.runtime) {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "commit folder-patch authority",
                "folder-patch authority plan belongs to another command runtime",
            ));
        }
        commit(plan, payload, &self.inner.state_root).map_err(commit_error)
    }
}

fn commit(
    plan: FolderPatchAuthorityPlan,
    payload: Vec<u8>,
    state_root: &std::path::Path,
) -> Result<FolderPatchAuthority, String> {
    let FolderPatchAuthorityPlan { runtime: _, contract, ids } = plan;
    let intent = peritus_workspace::folder_patch_action_intent(
        ids.actor,
        ids.action,
        ids.environment,
        ids.resource,
        payload,
    )
    .map_err(|error| format!("construct folder-patch intent: {error}"))?;
    if intent.capability_name != ids.capability {
        return Err("folder-patch capability differs from the C1 canonical intent".to_owned());
    }
    let digest = intent
        .digest(peritus_codec::CodecLimits::PRODUCTION)
        .map_err(|error| format!("digest folder-patch intent: {error}"))?;
    let capability_use = authority::capability_use(
        &ids,
        digest,
        OperationClass::WorkspaceMutation,
        RiskClass::ScopedWrite,
    )?;
    let directory = state_root.join("authority").join(identity::action_hex(ids.action));
    let path = directory.join("folder-patch.sqlite3");
    let label = "folder-patch-authority-store";
    let mut store = journal::open(&path, &ids, label)?;
    let kernel = kernel::commit(
        &mut store,
        label,
        &ids,
        &contract,
        &intent,
        &capability_use,
        AUTHORITY_WINDOW_MILLIS,
    )?;
    let (capability, lease) =
        lease::commit(&mut store, label, &ids, capability_use, AUTHORITY_WINDOW_MILLIS)?;
    let epoch = authority::allocate_epoch(&mut store)?;
    Ok(FolderPatchAuthority {
        intent,
        kernel,
        capability,
        lease,
        epoch,
        revision: ids.revision,
        session_id: ids.session,
    })
}

fn plan_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Apply, "plan folder-patch authority", detail)
}

fn commit_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Apply, "commit folder-patch authority", detail)
}
