//! Structured no-follow inspection of an immutable workspace snapshot.

use std::{fs, path::PathBuf};

use cap_std::fs::{Metadata as CapMetadata, MetadataExt as _};
use peritus_patch::WorkspacePath;

use crate::{ErrorCode, ReadOnlyWorkspace, RecoveryClass, WorkspaceError, WorkspaceOperation};

mod directory;
pub use directory::{DirectoryCursor, DirectoryPage};

/// Closed filesystem entry vocabulary returned by C1 inspection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkspaceEntryKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symlink or special node observed as a child without opening or following it.
    Other,
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
        Ok(metadata_from(path.clone(), &metadata))
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
        self.list_directory_cancellable(path, || false)?
            .ok_or_else(|| invalid("workspace directory listing was cancelled without a request"))
    }

    /// Lists direct children in canonical order while checking cancellation during enumeration.
    ///
    /// # Errors
    /// Returns the same no-follow inspection failures as `list_directory`.
    pub fn list_directory_cancellable(
        &self,
        path: Option<&WorkspacePath>,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Option<Vec<DirectoryEntry>>, WorkspaceError> {
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
            if cancelled() {
                return Ok(None);
            }
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
            entries.push(DirectoryEntry(metadata_from(child, &metadata)));
        }
        entries.sort_unstable_by(|left, right| left.0.path.cmp(&right.0.path));
        if cancelled() { Ok(None) } else { Ok(Some(entries)) }
    }

    /// Visits direct children in canonical order and permits cancellation between entries.
    ///
    /// Each bounded page is sorted before callbacks, preserving `list_directory` ordering without
    /// retaining the complete directory in the caller.
    ///
    /// # Errors
    /// Returns the same no-follow inspection failures as `list_directory`.
    pub fn visit_directory(
        &self,
        path: Option<&WorkspacePath>,
        mut cancelled: impl FnMut() -> bool,
        mut visit: impl FnMut(DirectoryEntry) -> bool,
    ) -> Result<bool, WorkspaceError> {
        let mut cursor = None;
        loop {
            let Some(page) =
                self.list_directory_page_cancellable(path, cursor.as_ref(), &mut cancelled)?
            else {
                return Ok(false);
            };
            let (entries, next) = page.into_parts();
            for entry in entries {
                if cancelled() || !visit(entry) {
                    return Ok(false);
                }
            }
            let Some(next) = next else { return Ok(true) };
            cursor = Some(next);
        }
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
        self.read_file_selection(path, crate::FileReadSelection::all(), maximum_bytes)
            .map(crate::InspectedFile::into_bytes)
    }

    /// Reads an exact whole-file, byte-range, or line selection while hashing the complete source.
    ///
    /// # Errors
    /// Returns a typed failure for invalid bounds, unsafe paths, source drift, or I/O failure.
    pub fn read_file_selection(
        &self,
        path: &WorkspacePath,
        selection: crate::FileReadSelection,
        maximum_bytes: u64,
    ) -> Result<crate::InspectedFile, WorkspaceError> {
        self.file_inspection().read_file(path, selection, maximum_bytes)
    }

    /// Reads an exact selection while checking cancellation between source chunks.
    ///
    /// # Errors
    /// Returns a typed failure for invalid bounds, unsafe paths, drift, or I/O failure.
    pub fn read_file_selection_cancellable(
        &self,
        path: &WorkspacePath,
        selection: crate::FileReadSelection,
        maximum_bytes: u64,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Option<crate::InspectedFile>, WorkspaceError> {
        self.file_inspection().read_file_cancellable(path, selection, maximum_bytes, cancelled)
    }

    /// Streams one exact no-follow file with cancellation checks between chunks.
    ///
    /// # Errors
    /// Returns a typed failure for an unsafe, changed, or unavailable source.
    pub fn scan_file_chunks(
        &self,
        path: &WorkspacePath,
        cancelled: impl FnMut() -> bool,
        visit: impl FnMut(u64, &[u8]),
    ) -> Result<Option<(u64, peritus_types::Sha256Digest)>, WorkspaceError> {
        self.file_inspection().scan_file_chunks(path, cancelled, visit)
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

fn metadata_from(path: WorkspacePath, metadata: &fs::Metadata) -> WorkspaceMetadata {
    let kind = if metadata.is_file() {
        WorkspaceEntryKind::File
    } else if metadata.is_dir() {
        WorkspaceEntryKind::Directory
    } else {
        WorkspaceEntryKind::Other
    };
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        kind == WorkspaceEntryKind::File && metadata.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    WorkspaceMetadata {
        path,
        kind,
        size: if kind == WorkspaceEntryKind::File { metadata.len() } else { 0 },
        executable,
    }
}

fn metadata_from_cap(path: WorkspacePath, metadata: &CapMetadata) -> WorkspaceMetadata {
    let kind = if metadata.is_file() {
        WorkspaceEntryKind::File
    } else if metadata.is_dir() {
        WorkspaceEntryKind::Directory
    } else {
        WorkspaceEntryKind::Other
    };
    #[cfg(unix)]
    let executable = kind == WorkspaceEntryKind::File && metadata.mode() & 0o111 != 0;
    #[cfg(not(unix))]
    let executable = false;
    WorkspaceMetadata {
        path,
        kind,
        size: if kind == WorkspaceEntryKind::File { metadata.len() } else { 0 },
        executable,
    }
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
