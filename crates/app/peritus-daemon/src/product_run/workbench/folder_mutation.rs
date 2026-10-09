//! Authenticated reviewed folder patches through committed G4/B0/B1 authority and C1 ownership.

use super::{ControlError, ControlStore, Error, ProductRunService};
use peritus_app_protocol::WorkbenchCommand;
use peritus_patch::PatchSet;
use peritus_product_runner::control::{ConversationId, PermissionCapability};
use peritus_product_runner::{CommandRuntime, FolderPatchAuthorityPlanRequest};
use peritus_types::{ActionId, ActorId, EnvironmentId, ResourceId, RunId, SessionId, WorkspaceId};
use peritus_workspace::{
    FolderIdentity, FolderMutationGateway, FolderMutationOpenRequest, FolderMutationOutcome,
};

mod init;

impl ProductRunService {
    pub(crate) async fn workbench_folder_command(
        &self,
        actor: ActorId,
        session: SessionId,
        command: &WorkbenchCommand,
    ) -> peritus_app_protocol::AppResponsePayload {
        match command.intent() {
            peritus_app_protocol::WorkbenchIntent::ApplyInitDiff(_) => {
                self.apply_workbench_init(actor, session, command).await
            }
            peritus_app_protocol::WorkbenchIntent::ApplyRewind(_) => {
                self.apply_workbench_rewind(actor, session, command).await
            }
            _ => super::error_response(ControlError::InvalidInput.into()),
        }
    }

    /// The caller holds the serialized control owner through authorization, effect, and receipt.
    pub(super) fn apply_authorized_folder_patch(
        &self,
        store: &ControlStore,
        actor: ActorId,
        session: SessionId,
        command: &WorkbenchCommand,
        patch: PatchSet,
        current_revision: u64,
    ) -> Result<FolderMutationOutcome, Error> {
        self.require_folder_write(store, actor, command, current_revision)?;
        if patch.expected_revision().get() != command.expected_revision() {
            return Err(ControlError::StaleRevision.into());
        }
        let workspace = command.query().workspace();
        let root = self.inner.workspaces.get(&workspace).ok_or(ControlError::ScopeMismatch)?;
        if let Some(folder) = self.inner.folders.get(&workspace) {
            folder.verify().map_err(|_| ControlError::ScopeMismatch)?;
            if !folder.writable() {
                return Err(Error::PermissionDenied);
            }
        }
        let identity = FolderIdentity::observe(root)?;
        let resource = folder_resource_id(workspace)?;
        let environment = folder_environment_id(&identity)?;
        let action = ActionId::new(command.operation().into_bytes())
            .map_err(|_| ControlError::InvalidInput)?;
        let run =
            RunId::new(command.operation().into_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let state =
            self.inner.directory.join("workbench-folder-commands").join(hex_id(run.as_bytes()));
        let runtime = CommandRuntime::open(state, root, run, self.inner.processes.clone())?;
        let plan = runtime.plan_folder_patch_authority(FolderPatchAuthorityPlanRequest::new(
            actor,
            session,
            action,
            workspace,
            resource,
            environment,
            patch.expected_generation(),
            patch.expected_revision(),
        ))?;
        let mut owner = FolderMutationGateway::open(FolderMutationOpenRequest::new(
            identity,
            resource,
            environment,
            plan.revision(),
            plan.lease_holder(),
            self.inner.directory.join("workbench-folder-transactions"),
        ))?;
        let payload = owner.authorization_payload(&patch)?;
        let authority = runtime.commit_folder_patch_authority(plan, payload)?;
        #[cfg(test)]
        super::checkpoints::obstruct_folder_patch(command, owner.transaction_namespace(), &patch)?;
        owner.apply_patch(&authority.request(), patch).map_err(Into::into)
    }

    pub(super) fn require_folder_write(
        &self,
        store: &ControlStore,
        actor: ActorId,
        command: &WorkbenchCommand,
        current_revision: u64,
    ) -> Result<(), Error> {
        self.control_workspace(command.query())?;
        let id = ConversationId::new(command.query().conversation().into_bytes())?;
        let record = store.load(id)?.ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != command.query().workspace().as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        if record.revision() != current_revision {
            return Err(ControlError::StaleRevision.into());
        }
        let host = self.permission_host(command.query().workspace())?;
        if !store
            .permission_policy(command.query().workspace())?
            .effective(host, PermissionCapability::Write)
        {
            return Err(Error::PermissionDenied);
        }
        Ok(())
    }
}

pub(super) fn folder_resource_id(workspace: WorkspaceId) -> Result<ResourceId, Error> {
    ResourceId::new(bound_id(b"peritus-folder-resource-v1\0", workspace.as_bytes()))
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn folder_environment_id(identity: &FolderIdentity) -> Result<EnvironmentId, Error> {
    EnvironmentId::new(bound_id(b"peritus-folder-environment-v1\0", identity.digest().as_bytes()))
        .map_err(|_| ControlError::InvalidInput.into())
}

fn bound_id(domain: &[u8], value: &[u8]) -> [u8; 16] {
    let mut input = domain.to_vec();
    input.extend_from_slice(value);
    let digest = peritus_codec::sha256(&input);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] |= 1;
    bytes
}

fn hex_id(bytes: &[u8; 16]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [char::from(DIGITS[usize::from(byte >> 4)]), char::from(DIGITS[usize::from(byte & 15)])]
        })
        .collect()
}
