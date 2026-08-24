//! Structured no-follow inspection of an immutable workspace snapshot.

use std::{fs, io::Read, path::PathBuf};

use peritus_patch::WorkspacePath;

use crate::{ErrorCode, ReadOnlyWorkspace, RecoveryClass, WorkspaceError, WorkspaceOperation};

/// Hard maximum returned by one immutable file read.
pub const MAX_INSPECTION_FILE_BYTES: u64 = 8 * 1_024 * 1_024;

/// Closed filesystem entry vocabulary returned by C1 inspection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkspaceEntryKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
}

/// Stable metadata for one no-follow workspace entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceMetadata {
    path: WorkspacePath,
    kind: WorkspaceEntryKind,
    size: u64,
    executable: bool,
}

impl WorkspaceMetadata {
    /// Returns the canonical workspace-relative path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }

    /// Returns the closed entry kind.
    #[must_use]
    pub const fn kind(&self) -> WorkspaceEntryKind {
        self.kind
    }

    /// Returns the exact byte size reported for a regular file.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Returns whether a regular file has any executable mode bit on Unix.
    #[must_use]
    pub const fn executable(&self) -> bool {
        self.executable
    }
}

/// One direct child returned by deterministic directory inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry(WorkspaceMetadata);

impl DirectoryEntry {
    /// Returns the child metadata.
    #[must_use]
    pub const fn metadata(&self) -> &WorkspaceMetadata {
        &self.0
    }
}

impl ReadOnlyWorkspace {
    /// Inspects one exact regular file or directory without following symlinks.
    ///
    /// # Errors
    /// Returns a typed failure for an absent, symlinked, special, or changed entry.
    pub fn metadata(&self, path: &WorkspacePath) -> Result<WorkspaceMetadata, WorkspaceError> {
        let target = checked_target(self, path)?;
        let metadata = fs::symlink_metadata(&target).map_err(|_| {
            inspect_error(
                ErrorCode::Indeterminate,
                RecoveryClass::Reobserve,
                "workspace entry metadata could not be observed",
            )
        })?;
        metadata_from(path.clone(), &metadata)
    }

    /// Lists direct children in canonical path order without following symlinks.
    ///
    /// `None` selects the workspace root. Protected metadata entries are not exposed.
    ///
    /// # Errors
    /// Returns a typed failure for a non-directory, symlink, non-UTF-8 child, or I/O failure.
    pub fn list_directory(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<Vec<DirectoryEntry>, WorkspaceError> {
        let directory = match path {
            Some(path) => checked_target(self, path)?,
            None => self.root().to_path_buf(),
        };
        let metadata = fs::symlink_metadata(&directory).map_err(|_| {
            inspect_error(
                ErrorCode::Indeterminate,
                RecoveryClass::Reobserve,
                "workspace directory metadata could not be observed",
            )
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid("workspace inspection target is not a no-follow directory"));
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|_| inspect_io())? {
            let entry = entry.map_err(|_| inspect_io())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("workspace contains a non-UTF-8 entry name"))?;
            if protected_component(&name) {
                continue;
            }
            let value = path.map_or_else(|| name.clone(), |parent| format!("{parent}/{name}"));
            let child = WorkspacePath::new(value)
                .map_err(|_| invalid("workspace child path is not representable"))?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|_| inspect_io())?;
            entries.push(DirectoryEntry(metadata_from(child, &metadata)?));
        }
        entries.sort_unstable_by(|left, right| left.0.path.cmp(&right.0.path));
        Ok(entries)
    }

    /// Reads one exact regular file without following symlinks.
    ///
    /// # Errors
    /// Returns a typed failure for invalid bounds, a non-file, symlink, drift, or I/O failure.
    pub fn read_file(
        &self,
        path: &WorkspacePath,
        maximum_bytes: u64,
    ) -> Result<Vec<u8>, WorkspaceError> {
        if maximum_bytes == 0 || maximum_bytes > MAX_INSPECTION_FILE_BYTES {
            return Err(invalid("workspace read bound is zero or exceeds the C1 maximum"));
        }
        let target = checked_target(self, path)?;
        let before = fs::symlink_metadata(&target).map_err(|_| inspect_io())?;
        if !before.is_file() || before.file_type().is_symlink() || before.len() > maximum_bytes {
            return Err(invalid("workspace read target is not a bounded no-follow regular file"));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(before.len()).unwrap_or(0));
        fs::File::open(&target)
            .map_err(|_| inspect_io())?
            .take(maximum_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| inspect_io())?;
        let after = fs::symlink_metadata(&target).map_err(|_| inspect_io())?;
        if bytes.len() as u64 != before.len()
            || before.len() != after.len()
            || after.file_type().is_symlink()
        {
            return Err(inspect_error(
                ErrorCode::Indeterminate,
                RecoveryClass::Reobserve,
                "workspace file changed during immutable inspection",
            ));
        }
        Ok(bytes)
    }
}

fn checked_target(
    workspace: &ReadOnlyWorkspace,
    path: &WorkspacePath,
) -> Result<PathBuf, WorkspaceError> {
    let mut current = workspace.root().to_path_buf();
    let components = path.as_str().split('/').collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|_| inspect_io())?;
        if metadata.file_type().is_symlink() || (index + 1 < components.len() && !metadata.is_dir())
        {
            return Err(invalid("workspace inspection refuses symlink or non-directory traversal"));
        }
    }
    Ok(current)
}

fn metadata_from(
    path: WorkspacePath,
    metadata: &fs::Metadata,
) -> Result<WorkspaceMetadata, WorkspaceError> {
    let kind = if metadata.is_file() {
        WorkspaceEntryKind::File
    } else if metadata.is_dir() {
        WorkspaceEntryKind::Directory
    } else {
        return Err(invalid("workspace inspection refuses symlinks and special nodes"));
    };
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        kind == WorkspaceEntryKind::File && metadata.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    Ok(WorkspaceMetadata {
        path,
        kind,
        size: if kind == WorkspaceEntryKind::File { metadata.len() } else { 0 },
        executable,
    })
}

fn protected_component(value: &str) -> bool {
    value.eq_ignore_ascii_case(".git")
        || value.eq_ignore_ascii_case(".peritus")
        || value.to_ascii_lowercase().starts_with(".peritus-txn-")
}

const fn invalid(detail: &'static str) -> WorkspaceError {
    inspect_error(ErrorCode::InvalidInput, RecoveryClass::CorrectRequest, detail)
}

const fn inspect_io() -> WorkspaceError {
    inspect_error(
        ErrorCode::Indeterminate,
        RecoveryClass::Reobserve,
        "workspace inspection could not establish a complete result",
    )
}

const fn inspect_error(
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: &'static str,
) -> WorkspaceError {
    WorkspaceError::new(code, WorkspaceOperation::Inspect, recovery, detail)
}
