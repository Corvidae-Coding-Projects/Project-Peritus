//! Recursive deterministic C1 inventory of the harness component root.

use peritus_patch::WorkspacePath;
use peritus_workspace::{ReadOnlyWorkspace, WorkspaceEntryKind};

use crate::domain::HarnessLimits;

use super::{ManifestError, ManifestErrorKind};

pub(super) fn component_inventory(
    workspace: &ReadOnlyWorkspace,
    limits: HarnessLimits,
) -> Result<Vec<WorkspacePath>, ManifestError> {
    let root = WorkspacePath::new(".peritus-harness/components".to_owned())
        .map_err(|_| invalid("component inventory root is not representable by C1"))?;
    let metadata = workspace.metadata(&root).map_err(workspace_error)?;
    if metadata.kind() != WorkspaceEntryKind::Directory {
        return Err(ManifestError::at(
            ManifestErrorKind::UnsafeEntry,
            root.as_str(),
            "component root is not a no-follow directory",
        ));
    }
    let max_entries = limits.max_components().saturating_mul(8).max(limits.max_components());
    let mut pending = vec![root];
    let mut files = Vec::new();
    let mut observed_entries = 0_u64;
    while let Some(directory) = pending.pop() {
        let entries = workspace.list_directory(Some(&directory)).map_err(workspace_error)?;
        for entry in entries.into_iter().rev() {
            observed_entries = observed_entries.saturating_add(1);
            if observed_entries > max_entries {
                return Err(ManifestError::new(
                    ManifestErrorKind::UnsafeEntry,
                    "component inventory exceeds the bounded entry ceiling",
                ));
            }
            let metadata = entry.metadata();
            match metadata.kind() {
                WorkspaceEntryKind::File => files.push(metadata.path().clone()),
                WorkspaceEntryKind::Directory => pending.push(metadata.path().clone()),
            }
        }
    }
    files.sort_unstable();
    if u64::try_from(files.len()).unwrap_or(u64::MAX) > limits.max_components() {
        return Err(ManifestError::new(
            ManifestErrorKind::UnsafeEntry,
            "component file inventory exceeds the component limit",
        ));
    }
    Ok(files)
}

fn workspace_error(error: impl core::fmt::Display) -> ManifestError {
    ManifestError::new(ManifestErrorKind::Workspace, error.to_string())
}

fn invalid(detail: &'static str) -> ManifestError {
    ManifestError::new(ManifestErrorKind::UnsafeEntry, detail)
}
