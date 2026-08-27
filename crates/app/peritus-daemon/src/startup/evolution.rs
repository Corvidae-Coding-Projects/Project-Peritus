//! Config-bound F0 production-pointer recovery and exact E1 revision resolution.

use std::collections::BTreeMap;

use peritus_evolution::{ProductionHarnessBinding, ProductionHarnessState};
use peritus_journal::SqliteJournal;
use peritus_types::ProjectId;

use super::workspace::WorkspaceCatalog;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

/// Replayed production pointers for the exact configured project inventory.
pub(super) struct ProductionCatalog {
    pointers: BTreeMap<ProjectId, Option<ProductionHarnessState>>,
}

impl ProductionCatalog {
    pub(super) fn len(&self) -> usize {
        self.pointers.len()
    }
}

/// Replays every configured F0 pointer and resolves active bindings through E1.
pub(super) fn recover_production(
    journal: &SqliteJournal,
    config: &DaemonConfig,
    workspaces: &WorkspaceCatalog,
) -> Result<ProductionCatalog, DaemonError> {
    let mut pointers = BTreeMap::new();
    for project in config.projects() {
        let project_id = project.project_identity()?;
        let configured_workspaces = project.workspace_identities()?;
        let replay = peritus_evolution::recover_pointer(journal, project_id)
            .map_err(|error| evolution_error("recover production pointer", error))?;
        let state = replay.state().cloned();
        if let Some(state) = &state {
            verify_pointer(journal, project_id, &configured_workspaces, workspaces, state)?;
        }
        if pointers.insert(project_id, state).is_some() {
            return Err(corrupt("configured project identity is duplicated after validation"));
        }
    }
    Ok(ProductionCatalog { pointers })
}

fn verify_pointer(
    journal: &SqliteJournal,
    configured_project: ProjectId,
    configured_workspaces: &[peritus_types::WorkspaceId],
    workspaces: &WorkspaceCatalog,
    state: &ProductionHarnessState,
) -> Result<(), DaemonError> {
    let binding = state.current();
    let revision = binding.revision();
    let harness_identity = binding.harness_revision();
    let installed = binding.installed_snapshot();
    let project_mismatch = state.project_id() != configured_project;
    let workspace_mismatch = !configured_workspaces.contains(&revision.workspace_id())
        || !workspaces.contains(revision.workspace_id());
    let harness_mismatch = revision.harness_id() != harness_identity.harness_id();
    let snapshot_mismatch = installed.workspace_id() != revision.workspace_id()
        || installed.generation() != revision.workspace_generation()
        || installed.revision() != revision.workspace_revision();
    if project_mismatch || workspace_mismatch || harness_mismatch || snapshot_mismatch {
        return Err(corrupt(
            "production pointer differs from its configured project, workspace, or snapshot",
        ));
    }
    verify_harness_revision(journal, binding)
}

fn verify_harness_revision(
    journal: &SqliteJournal,
    binding: ProductionHarnessBinding,
) -> Result<(), DaemonError> {
    let expected = binding.harness_revision();
    let replay = peritus_harness::load_harness_replay(journal, expected.harness_id())
        .map_err(|error| harness_error("recover production harness", error))?;
    let state = replay
        .rebuild()
        .map_err(|error| harness_error("rebuild production harness", error))?
        .ok_or_else(|| corrupt("production pointer names an absent E1 harness aggregate"))?;
    let resolved = state.history().revision(expected.digest()).ok_or_else(|| {
        corrupt("production pointer names an E1 revision outside retained history")
    })?;
    if resolved.identity() != expected {
        return Err(corrupt("production pointer E1 revision identity does not resolve exactly"));
    }
    Ok(())
}

fn evolution_error(
    operation: &'static str,
    error: peritus_evolution::EvolutionError,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        operation,
        "F0 production-pointer replay failed",
        error,
    )
}

fn harness_error(operation: &'static str, error: peritus_harness::DurabilityError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        operation,
        "E1 production-harness replay failed",
        error,
    )
}

fn corrupt(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "verify production harness pointer",
        detail,
    )
}
