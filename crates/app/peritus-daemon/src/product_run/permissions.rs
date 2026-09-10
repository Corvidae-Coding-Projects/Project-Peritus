//! Host-intersected permission inspection and admission checks.

use super::ProductRunService;
use crate::{DaemonComponents, product_control::ControlStoreError as Error};
use peritus_app_protocol::{
    AppProtocolError, AppRequestPayload, AppResponsePayload, WorkbenchIntent,
    WorkbenchPermissionCapability, WorkbenchPermissionEntry, WorkbenchPermissionProvenance,
    WorkbenchPermissions, WorkbenchQuery, WorkbenchWorkspaceTrust,
};
use peritus_product_runner::control::{
    ControlError, ConversationBranchMode, ConversationId, HostPermissions, PermissionCapability,
    PermissionPolicy,
};
use peritus_types::{ActorId, RunId, WorkspaceId};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct HostPermissionCatalog {
    entries: BTreeMap<WorkspaceId, HostPermissionEntry>,
}

#[derive(Clone, Copy)]
struct HostPermissionEntry {
    permissions: HostPermissions,
    trust: WorkbenchWorkspaceTrust,
}

impl HostPermissionCatalog {
    pub(super) fn new(
        components: &DaemonComponents,
        workspaces: &crate::startup::workspace::WorkspaceCatalog,
    ) -> Self {
        let network = !components.providers().keys().is_empty();
        let entries = workspaces
            .roots()
            .into_keys()
            .map(|workspace| {
                let folder = workspaces.folders().get(&workspace);
                let (write, trust) =
                    folder.map_or((true, WorkbenchWorkspaceTrust::Managed), |folder| {
                        if folder.writable() {
                            (true, WorkbenchWorkspaceTrust::DirectWritable)
                        } else {
                            (false, WorkbenchWorkspaceTrust::DirectReadOnly)
                        }
                    });
                (
                    workspace,
                    HostPermissionEntry { permissions: host_permissions(write, network), trust },
                )
            })
            .collect();
        Self { entries }
    }

    #[cfg(test)]
    pub(super) fn managed(
        workspaces: impl IntoIterator<Item = WorkspaceId>,
        network: bool,
    ) -> Self {
        let permissions = host_permissions(true, network);
        let entries = workspaces
            .into_iter()
            .map(|workspace| {
                (
                    workspace,
                    HostPermissionEntry { permissions, trust: WorkbenchWorkspaceTrust::Managed },
                )
            })
            .collect();
        Self { entries }
    }

    fn get(&self, workspace: WorkspaceId) -> Result<HostPermissionEntry, Error> {
        self.entries.get(&workspace).copied().ok_or_else(|| ControlError::ScopeMismatch.into())
    }
}

const fn host_permissions(write: bool, network: bool) -> HostPermissions {
    let permissions = if write {
        HostPermissions::all()
    } else {
        HostPermissions::all().without(PermissionCapability::Write)
    };
    if network { permissions } else { permissions.without(PermissionCapability::Network) }
}

impl ProductRunService {
    /// Central A3 admission map for requests that directly inspect or affect host resources.
    pub(crate) fn authorize_workbench_request(
        &self,
        actor: ActorId,
        request: &AppRequestPayload,
    ) -> Result<(), AppProtocolError> {
        let Some((query, required)) = request_permissions(request) else {
            return Ok(());
        };
        self.require_workspace_permissions(actor, query, required)
            .map_err(super::workbench::error_value)
    }

    pub(crate) fn workbench_permissions(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query).and_then(|()| {
            let id = ConversationId::new(query.conversation().into_bytes())?;
            let mut host = self.inner.host_permissions.get(query.workspace())?;
            self.with_controls(false, |store| {
                let record = store.load(id)?.ok_or(ControlError::NotFound)?;
                if record.owner_bytes() != actor.as_bytes()
                    || record.workspace_bytes() != query.workspace().as_bytes()
                {
                    return Err(ControlError::ScopeMismatch.into());
                }
                host.permissions = branch_permissions(host.permissions, store.branch(id)?.as_ref());
                let policy = store.permission_policy(query.workspace())?;
                projection(query, record.revision(), policy, host)
            })
        });
        result
            .map_or_else(super::workbench::error_response, AppResponsePayload::WorkbenchPermissions)
    }

    pub(super) fn permission_host(&self, workspace: WorkspaceId) -> Result<HostPermissions, Error> {
        Ok(self.inner.host_permissions.get(workspace)?.permissions)
    }

    /// Authenticates a conversation/workspace pair and loads its current effective capabilities.
    pub(super) fn workspace_permissions(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
    ) -> Result<HostPermissions, Error> {
        self.control_workspace(query)?;
        let conversation = ConversationId::new(query.conversation().into_bytes())?;
        let host = self.inner.host_permissions.get(query.workspace())?.permissions;
        self.with_controls(false, |store| {
            let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
            if record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != query.workspace().as_bytes()
            {
                return Err(ControlError::ScopeMismatch.into());
            }
            let host = branch_permissions(host, store.branch(conversation)?.as_ref());
            Ok(store.permission_policy(query.workspace())?.effective_permissions(host))
        })
    }

    pub(super) fn require_workspace_permissions(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
        required: &[PermissionCapability],
    ) -> Result<(), Error> {
        let permissions = self.workspace_permissions(actor, query)?;
        if required.iter().all(|capability| permissions.allows(*capability)) {
            Ok(())
        } else {
            Err(Error::PermissionDenied)
        }
    }

    pub(super) fn require_run_permissions(
        &self,
        run: RunId,
        required: &[PermissionCapability],
    ) -> Result<(), Error> {
        let permissions = self.effective_permissions(run)?;
        if required.iter().all(|capability| permissions.allows(*capability)) {
            Ok(())
        } else {
            Err(Error::PermissionDenied)
        }
    }

    /// Rechecks the latest durable policy at an actual provider/tool admission boundary.
    pub(super) fn permission_allows(
        &self,
        run: RunId,
        capability: PermissionCapability,
    ) -> Result<bool, Error> {
        Ok(self.effective_permissions(run)?.allows(capability))
    }

    /// Loads one current host-intersected permission snapshot for an execution boundary.
    pub(super) fn effective_permissions(&self, run: RunId) -> Result<HostPermissions, Error> {
        let records = self
            .inner
            .records
            .read()
            .map_err(|_| Error::Corrupt("product run registry lock poisoned"))?;
        let record = records.get(&run).ok_or(ControlError::NotFound)?;
        let Some(start) =
            record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
        else {
            // Ordinary product runs predate workbench policy and retain the shared
            // ConversationView default. Only a present governed binding may narrow it.
            return Ok(HostPermissions::all());
        };
        let workspace =
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let conversation = start.conversation();
        let host = self.inner.host_permissions.get(workspace)?.permissions;
        drop(records);
        self.with_controls(false, |store| {
            let host = branch_permissions(host, store.branch(conversation)?.as_ref());
            Ok(store.permission_policy(workspace)?.effective_permissions(host))
        })
    }
}

const fn request_permissions(
    request: &AppRequestPayload,
) -> Option<(WorkbenchQuery, &'static [PermissionCapability])> {
    use PermissionCapability::Read;
    let (query, required): (_, &'static [_]) = match request {
        AppRequestPayload::PreviewWorkbenchFileImport(value) => {
            (value.selection().query(), &[Read])
        }
        AppRequestPayload::PreviewWorkbenchFile(value) => (value.query(), &[Read]),
        AppRequestPayload::QueryWorkbenchFiles(value) => (value.query(), &[Read]),
        AppRequestPayload::QueryWorkbenchImages(value) => (value.query(), &[Read]),
        AppRequestPayload::PreviewWorkbenchImage(value) => (value.query(), &[Read]),
        AppRequestPayload::InspectWorkbenchCheckpoint(value)
        | AppRequestPayload::PreviewWorkbenchRewind(value) => (value.query(), &[Read]),
        AppRequestPayload::DiscoverInit(value) => (value.query(), &[Read]),
        AppRequestPayload::WorkbenchCommand(command) => {
            let required = command_permissions(command.intent());
            if required.is_empty() {
                return None;
            }
            (command.query(), required)
        }
        _ => return None,
    };
    Some((query, required))
}

pub(super) const fn command_permissions(
    intent: &WorkbenchIntent,
) -> &'static [PermissionCapability] {
    use PermissionCapability::{Network, Process, Read, Write};
    match intent {
        WorkbenchIntent::CreateCheckpoint(_) | WorkbenchIntent::ForkConversation(_) => &[Read],
        WorkbenchIntent::ApplyRewind(preview) => match preview.request().mode() {
            peritus_app_protocol::WorkbenchRewindMode::ConversationOnly => &[Read],
            peritus_app_protocol::WorkbenchRewindMode::FilesOnly
            | peritus_app_protocol::WorkbenchRewindMode::Combined => &[Read, Write],
        },
        WorkbenchIntent::ApplyInitDiff(_) => &[Read, Write],
        WorkbenchIntent::AttachFile { .. }
        | WorkbenchIntent::AttachFileImport { .. }
        | WorkbenchIntent::AttachImage { .. } => &[Read],
        WorkbenchIntent::StartPreview(_)
        | WorkbenchIntent::InteractPreview { .. }
        | WorkbenchIntent::CapturePreview(_) => &[Read, Write, Process, Network],
        // Permission inspection/change and owned cleanup/observation stay available.
        WorkbenchIntent::SetPermissions(_)
        | WorkbenchIntent::StopPreview { .. }
        | WorkbenchIntent::CheckPreviewBehavior { .. }
        | WorkbenchIntent::AddArtifactFeedback { .. } => &[],
        _ => &[],
    }
}

fn branch_permissions(
    host: HostPermissions,
    branch: Option<&peritus_product_runner::control::ConversationBranch>,
) -> HostPermissions {
    if branch
        .is_some_and(|branch| branch.mode() == ConversationBranchMode::ReadOnlyCurrentWorkspace)
    {
        host.without(PermissionCapability::Write).without(PermissionCapability::Process)
    } else {
        host
    }
}

fn projection(
    query: WorkbenchQuery,
    conversation_revision: u64,
    policy: PermissionPolicy,
    host: HostPermissionEntry,
) -> Result<WorkbenchPermissions, Error> {
    let entries: [Result<WorkbenchPermissionEntry, Error>; 4] = WorkbenchPermissionCapability::ALL
        .map(|capability| {
            let domain = domain_capability(capability);
            let host_allowed = host.permissions.allows(domain);
            let effective = policy.effective(host.permissions, domain);
            let provenance = if policy.requested(domain) {
                match capability {
                    WorkbenchPermissionCapability::Read | WorkbenchPermissionCapability::Write => {
                        WorkbenchPermissionProvenance::WorkspaceHostPolicy
                    }
                    WorkbenchPermissionCapability::Process => {
                        WorkbenchPermissionProvenance::ToolHostPolicy
                    }
                    WorkbenchPermissionCapability::Network => {
                        WorkbenchPermissionProvenance::ProviderHostPolicy
                    }
                }
            } else {
                WorkbenchPermissionProvenance::UserRestriction
            };
            WorkbenchPermissionEntry::new(
                capability,
                host_allowed,
                effective,
                provenance,
                matches!(
                    capability,
                    WorkbenchPermissionCapability::Write | WorkbenchPermissionCapability::Process
                ),
            )
            .map_err(|_| Error::from(ControlError::InvalidInput))
        });
    let entries: Vec<_> = entries.into_iter().collect::<Result<_, _>>()?;
    let entries = entries.try_into().map_err(|_| Error::from(ControlError::InvalidInput))?;
    WorkbenchPermissions::new(query, conversation_revision, policy.revision(), host.trust, entries)
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) const fn domain_capability(
    capability: WorkbenchPermissionCapability,
) -> PermissionCapability {
    match capability {
        WorkbenchPermissionCapability::Read => PermissionCapability::Read,
        WorkbenchPermissionCapability::Write => PermissionCapability::Write,
        WorkbenchPermissionCapability::Process => PermissionCapability::Process,
        WorkbenchPermissionCapability::Network => PermissionCapability::Network,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::{
        ControlOperationId, ConversationId as PublicConversationId, WorkbenchRewindMode,
        WorkbenchRewindPreview, WorkbenchRewindRequest,
    };

    fn rewind(mode: WorkbenchRewindMode) -> WorkbenchIntent {
        let query = WorkbenchQuery::new(
            PublicConversationId::new([1; 16]).unwrap(),
            WorkspaceId::new([2; 16]).unwrap(),
        );
        let mut request =
            WorkbenchRewindRequest::new(query, 1, ControlOperationId::new([3; 16]).unwrap())
                .unwrap();
        if mode != WorkbenchRewindMode::FilesOnly {
            request = request
                .with_branch(mode, PublicConversationId::new([4; 16]).unwrap(), None)
                .unwrap();
        }
        WorkbenchIntent::ApplyRewind(
            WorkbenchRewindPreview::new(request, Vec::new(), Vec::new(), Vec::new()).unwrap(),
        )
    }

    #[test]
    fn rewind_permissions_distinguish_logical_and_filesystem_effects() {
        assert_eq!(
            command_permissions(&rewind(WorkbenchRewindMode::ConversationOnly)),
            &[PermissionCapability::Read]
        );
        for mode in [WorkbenchRewindMode::FilesOnly, WorkbenchRewindMode::Combined] {
            assert_eq!(
                command_permissions(&rewind(mode)),
                &[PermissionCapability::Read, PermissionCapability::Write]
            );
        }
    }
}
