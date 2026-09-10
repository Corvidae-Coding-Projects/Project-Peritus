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

#[cfg(test)]
mod tests {
    use std::fs;

    use peritus_patch::{
        FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage, WorkspacePath,
    };
    use peritus_types::{ActionId, ActorId, Generation, RevisionNumber, RunId, WorkspaceId};
    use peritus_workspace::{FolderIdentity, FolderMutationGateway, FolderMutationOpenRequest};

    use super::*;

    #[test]
    fn real_authority_applies_only_the_exact_c1_folder_patch_bytes() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let folder = temporary.path().join("registered-folder");
        fs::create_dir(&folder).expect("registered folder");
        fs::write(folder.join("AGENTS.md"), b"observed instructions\n").expect("preimage");
        let runtime = CommandRuntime::open_for_test(
            &folder,
            RunId::new([41; 16]).expect("command runtime run"),
        );
        let actor = ActorId::new([42; 16]).expect("actor");
        let session = SessionId::new([43; 16]).expect("session");
        let action = ActionId::new([44; 16]).expect("action");
        let workspace = WorkspaceId::new([45; 16]).expect("workspace");
        let resource = ResourceId::new([46; 16]).expect("resource");
        let environment = EnvironmentId::new([47; 16]).expect("environment");
        let generation = Generation::first();
        let revision = RevisionNumber::first();
        let command_ordinal = runtime.inner.state.lock().expect("runtime state").next_ordinal;
        let plan = runtime
            .plan_folder_patch_authority(FolderPatchAuthorityPlanRequest::new(
                actor,
                session,
                action,
                workspace,
                resource,
                environment,
                generation,
                revision,
            ))
            .expect("folder-patch plan");
        assert_eq!(
            runtime.inner.state.lock().expect("runtime state").next_ordinal,
            command_ordinal
        );

        let identity = FolderIdentity::observe(&folder).expect("folder identity");
        let mut gateway = FolderMutationGateway::open(FolderMutationOpenRequest::new(
            identity,
            plan.resource_id(),
            plan.environment_id(),
            plan.revision(),
            plan.lease_holder(),
            temporary.path().join("transactions"),
        ))
        .expect("folder mutation gateway");
        let operation = PatchOperation::replace(
            WorkspacePath::new("AGENTS.md").expect("workspace path"),
            Preimage::from_bytes(b"observed instructions\n", FileMode::Regular),
            FinalFile::new(
                b"exact approved bytes\n".to_vec(),
                FileMode::Regular,
                LineEndingPolicy::Preserve,
            )
            .expect("postimage"),
        )
        .expect("replacement");
        let patch = PatchSet::new(workspace, generation, revision, vec![operation]).expect("patch");
        let payload = gateway.authorization_payload(&patch).expect("C1 payload");
        let authority = runtime
            .commit_folder_patch_authority(plan, payload)
            .expect("committed folder-patch authority");

        let outcome =
            gateway.apply_patch(&authority.request(), patch).expect("authorized folder patch");

        assert_eq!(outcome.action_id(), action);
        assert_eq!(
            fs::read(folder.join("AGENTS.md")).expect("postimage"),
            b"exact approved bytes\n"
        );
        assert!(!folder.join(".git").exists());
    }

    #[test]
    fn plan_cannot_be_committed_by_another_runtime_with_the_same_run_id() {
        let first_root = tempfile::tempdir().expect("first workspace");
        let second_root = tempfile::tempdir().expect("second workspace");
        let run = RunId::new([51; 16]).expect("shared nominal run");
        let first = CommandRuntime::open_for_test(first_root.path(), run);
        let second = CommandRuntime::open_for_test(second_root.path(), run);
        let plan = first
            .plan_folder_patch_authority(FolderPatchAuthorityPlanRequest::new(
                ActorId::new([52; 16]).expect("actor"),
                SessionId::new([53; 16]).expect("session"),
                ActionId::new([54; 16]).expect("action"),
                WorkspaceId::new([55; 16]).expect("workspace"),
                ResourceId::new([56; 16]).expect("resource"),
                EnvironmentId::new([57; 16]).expect("environment"),
                Generation::first(),
                RevisionNumber::first(),
            ))
            .expect("plan");

        let error = second
            .commit_folder_patch_authority(plan, b"wrong runtime".to_vec())
            .err()
            .expect("cross-runtime commit must fail");

        assert_eq!(error.kind(), ProductRunnerErrorKind::InvalidPrecondition);
        assert_eq!(error.operation(), "commit folder-patch authority");
    }
}
