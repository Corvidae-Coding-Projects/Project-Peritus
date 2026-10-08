//! Exact streamed workspace postimages; no later observation grants additional ownership.

use super::{CheckpointFileVersion, PreparedMutation, WorkspaceMutationKind};
use crate::{
    control::CheckpointFileMode,
    developer_tools::path::{checked, tool},
};
use peritus_agent::DeveloperLoopError;
use peritus_patch::WorkspacePath;
use peritus_workspace::{FolderIdentity, FolderInspection};
use std::fs;

pub(super) fn exact_file_receipt(
    root: &std::path::Path,
    path: &str,
) -> Result<PreparedMutation, DeveloperLoopError> {
    let target = checked(root, path, true)?;
    let before = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PreparedMutation {
                path: path.to_owned(),
                kind: WorkspaceMutationKind::File,
                owned_postchange: CheckpointFileVersion::Absent,
            });
        }
        Err(error) => return Err(tool(error.to_string())),
    };
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(tool("workspace receipt target is not a regular file or absent"));
    }
    let identity = FolderIdentity::observe(root).map_err(|error| tool(error.to_string()))?;
    let inspection = FolderInspection::open(&identity).map_err(|error| tool(error.to_string()))?;
    let workspace_path = WorkspacePath::new(path).map_err(|error| tool(error.to_string()))?;
    let (digest, bytes) = inspection
        .copy_snapshot(&workspace_path, &mut std::io::sink())
        .map_err(|error| tool(error.to_string()))?;
    let after = fs::symlink_metadata(&target).map_err(|error| tool(error.to_string()))?;
    if !after.is_file()
        || after.file_type().is_symlink()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || before.permissions() != after.permissions()
        || bytes != after.len()
    {
        return Err(tool("workspace target changed while recording its exact receipt"));
    }
    Ok(PreparedMutation {
        path: path.to_owned(),
        kind: WorkspaceMutationKind::File,
        owned_postchange: CheckpointFileVersion::present(digest, bytes, file_mode(&after)),
    })
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> CheckpointFileMode {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o111 == 0 {
        CheckpointFileMode::Regular
    } else {
        CheckpointFileMode::Executable
    }
}

#[cfg(not(unix))]
const fn file_mode(_metadata: &fs::Metadata) -> CheckpointFileMode {
    CheckpointFileMode::Regular
}
