//! Config-bound C1 registration installation and complete C0 catalog reconciliation.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use peritus_journal::{ApplicationWorkspaceState, SqliteJournal};
use peritus_types::WorkspaceId;
use peritus_workspace::{MAX_WORKSPACE_REGISTRATION_BYTES, WorkspaceRegistration};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

/// Exact immutable registrations admitted by this daemon instance.
pub struct WorkspaceCatalog {
    registrations: BTreeMap<WorkspaceId, WorkspaceRegistration>,
    folders: BTreeMap<WorkspaceId, crate::config::FolderDeclaration>,
}

impl WorkspaceCatalog {
    pub(crate) fn len(&self) -> usize {
        self.registrations.len()
    }

    pub(crate) fn contains(&self, workspace_id: WorkspaceId) -> bool {
        self.registrations.contains_key(&workspace_id)
    }

    pub(crate) fn root(&self, workspace_id: WorkspaceId) -> Option<&Path> {
        self.registrations
            .get(&workspace_id)
            .map(|registration| registration.worktree_manifest().root())
    }

    pub(crate) fn roots(&self) -> BTreeMap<WorkspaceId, std::path::PathBuf> {
        self.registrations
            .iter()
            .map(|(workspace_id, registration)| {
                (*workspace_id, registration.worktree_manifest().root().to_owned())
            })
            .chain(self.folders.iter().map(|(id, folder)| (*id, folder.root().to_owned())))
            .collect()
    }

    pub(crate) const fn folders(&self) -> &BTreeMap<WorkspaceId, crate::config::FolderDeclaration> {
        &self.folders
    }
}

pub(super) fn install_and_reconcile(
    journal: &mut SqliteJournal,
    config: &DaemonConfig,
) -> Result<WorkspaceCatalog, DaemonError> {
    let mut registrations = BTreeMap::new();
    for declaration in config.workspaces() {
        let path = declaration.registration_file();
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                crate::diagnostic::report(&format!(
                    "peritusd: workspace registration {} is unavailable; attempting journal recovery: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if !metadata.file_type().is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_WORKSPACE_REGISTRATION_BYTES as u64
        {
            crate::diagnostic::report(&format!(
                "peritusd: workspace registration {} is not a bounded regular file; attempting journal recovery",
                path.display()
            ));
            continue;
        }
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                crate::diagnostic::report(&format!(
                    "peritusd: workspace registration {} could not be read; attempting journal recovery: {error}",
                    path.display()
                ));
                continue;
            }
        };
        let registration = match WorkspaceRegistration::decode(&bytes) {
            Ok(registration) => registration,
            Err(error) => {
                crate::diagnostic::report(&format!(
                    "peritusd: workspace registration {} is invalid; attempting journal recovery: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if registrations.insert(registration.workspace_id(), registration).is_some() {
            return Err(invalid("workspace identity is configured more than once"));
        }
    }

    let referenced = config
        .projects()
        .iter()
        .map(crate::ProjectDeclaration::workspace_identities)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();
    for workspace_id in referenced.iter().copied() {
        if registrations.contains_key(&workspace_id) {
            continue;
        }
        let Some(durable) = journal.application_workspace(workspace_id).map_err(journal_error)?
        else {
            continue;
        };
        let registration =
            WorkspaceRegistration::from_application_workspace(&durable).map_err(workspace_error)?;
        registrations.insert(workspace_id, registration);
        crate::diagnostic::report(&format!(
            "peritusd: recovered configured workspace {workspace_id:?} from the durable journal"
        ));
    }
    let configured = registrations.keys().copied().collect::<BTreeSet<_>>();
    if referenced != configured {
        return Err(invalid(
            "configured projects and workspace registrations do not form an exact inventory",
        ));
    }

    for registration in registrations.values() {
        let durable = registration.durable_registration().map_err(journal_error)?;
        let installed = journal.register_application_workspace(durable).map_err(journal_error)?;
        if installed.state() != ApplicationWorkspaceState::Registered {
            journal
                .set_application_workspace_state(
                    installed.workspace_id(),
                    ApplicationWorkspaceState::Registered,
                )
                .map_err(journal_error)?;
        }
    }

    let mut after = None;
    loop {
        let page = journal.application_workspace_page(after, 256).map_err(journal_error)?;
        for row in page.workspaces() {
            let durable =
                WorkspaceRegistration::from_application_workspace(row).map_err(workspace_error)?;
            match registrations.get(&row.workspace_id()) {
                Some(configured) if configured == &durable => {}
                None => {
                    if row.state() != ApplicationWorkspaceState::Removed {
                        journal
                            .set_application_workspace_state(
                                row.workspace_id(),
                                ApplicationWorkspaceState::Removed,
                            )
                            .map_err(journal_error)?;
                    }
                }
                Some(_) => {
                    return Err(invalid(
                        "durable workspace catalog differs from the active configuration",
                    ));
                }
            }
        }
        let Some(next) = page.next_after() else {
            break;
        };
        after = Some(next);
    }
    let mut folders = BTreeMap::new();
    for folder in config.folders() {
        let id = folder.workspace_id()?;
        folder.verify()?;
        if registrations.contains_key(&id) || folders.insert(id, folder.clone()).is_some() {
            return Err(invalid("folder identity conflicts with another configured workspace"));
        }
    }
    Ok(WorkspaceCatalog { registrations, folders })
}

fn workspace_error(error: peritus_workspace::WorkspaceError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "reconcile workspace registration",
        error.to_string(),
        error,
    )
}

fn journal_error(error: peritus_journal::JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        error.operation(),
        error.to_string(),
        error,
    )
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "reconcile workspace registration",
        detail,
    )
}
