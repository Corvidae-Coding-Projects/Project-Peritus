//! Config-bound C1 registration installation and complete C0 catalog reconciliation.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use peritus_journal::{ApplicationWorkspaceState, SqliteJournal};
use peritus_types::WorkspaceId;
use peritus_workspace::{MAX_WORKSPACE_REGISTRATION_BYTES, WorkspaceRegistration};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

/// Exact immutable registrations admitted by this daemon instance.
pub(super) struct WorkspaceCatalog {
    registrations: BTreeMap<WorkspaceId, WorkspaceRegistration>,
}

impl WorkspaceCatalog {
    pub(super) fn len(&self) -> usize {
        self.registrations.len()
    }

    pub(super) fn contains(&self, workspace_id: WorkspaceId) -> bool {
        self.registrations.contains_key(&workspace_id)
    }
}

pub(super) fn install_and_reconcile(
    journal: &mut SqliteJournal,
    config: &DaemonConfig,
) -> Result<WorkspaceCatalog, DaemonError> {
    let mut registrations = BTreeMap::new();
    for declaration in config.workspaces() {
        let path = declaration.registration_file();
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| filesystem("inspect workspace registration file", error))?;
        if !metadata.file_type().is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_WORKSPACE_REGISTRATION_BYTES as u64
        {
            return Err(invalid("workspace registration must be a nonempty bounded regular file"));
        }
        let bytes = fs::read(path)
            .map_err(|error| filesystem("read workspace registration file", error))?;
        let registration = WorkspaceRegistration::decode(&bytes).map_err(workspace_error)?;
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
    let configured = registrations.keys().copied().collect::<BTreeSet<_>>();
    if referenced != configured {
        return Err(invalid(
            "configured projects and workspace registrations do not form an exact inventory",
        ));
    }

    for registration in registrations.values() {
        let durable = registration.durable_registration().map_err(journal_error)?;
        journal.register_application_workspace(durable).map_err(journal_error)?;
    }

    let mut after = None;
    loop {
        let page = journal.application_workspace_page(after, 256).map_err(journal_error)?;
        for row in page.workspaces() {
            let durable =
                WorkspaceRegistration::from_application_workspace(row).map_err(workspace_error)?;
            match registrations.get(&row.workspace_id()) {
                Some(configured) if configured == &durable => {}
                None if row.state() == ApplicationWorkspaceState::Removed => {}
                Some(_) | None => {
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
    Ok(WorkspaceCatalog { registrations })
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

fn filesystem(operation: &'static str, error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Operator,
        operation,
        "workspace registration file cannot be read safely",
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
