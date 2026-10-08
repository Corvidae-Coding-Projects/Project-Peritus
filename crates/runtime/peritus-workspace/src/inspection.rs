//! Structured no-follow inspection of an immutable workspace snapshot.

use std::{fs, path::PathBuf};

use peritus_patch::WorkspacePath;

use crate::{
    ErrorCode, FolderInspection, ReadOnlyWorkspace, RecoveryClass, WorkspaceError,
    WorkspaceOperation,
};

/// Historical buffer ceiling retained for source compatibility; reads use caller-owned capacity.
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
    pub(crate) const fn inspected(
        path: WorkspacePath,
        kind: WorkspaceEntryKind,
        size: u64,
        executable: bool,
    ) -> Self {
        Self { path, kind, size, executable }
    }

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

impl FolderInspection {
    /// Resolves one existing directory beneath this exact root without following any path component.
    /// `None` selects the root itself.
    ///
    /// # Errors
    /// Rejects missing, non-directory, linked, replaced, or canonically escaping selections.
    pub fn resolve_directory(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<PathBuf, WorkspaceError> {
        let reopened = Self::open(self.identity())?;
        let root = reopened.identity().root();
        let Some(path) = path else {
            return Ok(root.to_owned());
        };
        if reopened.metadata(path)?.kind() != WorkspaceEntryKind::Directory {
            return Err(directory_invalid("selected workspace path is not a directory"));
        }
        let candidate = root.join(path.as_str());
        let canonical = fs::canonicalize(&candidate).map_err(directory_io)?;
        if canonical != candidate || !canonical.starts_with(root) {
            return Err(directory_invalid(
                "selected workspace directory does not resolve exactly beneath its bound root",
            ));
        }
        if reopened.metadata(path)?.kind() != WorkspaceEntryKind::Directory {
            return Err(directory_invalid("selected workspace directory changed during resolution"));
        }
        Ok(canonical)
    }
}

impl ReadOnlyWorkspace {
    /// Resolves one existing directory in this immutable snapshot without following links.
    /// `None` selects the snapshot root.
    ///
    /// # Errors
    /// Rejects missing, non-directory, linked, replaced, or canonically escaping selections.
    pub fn resolve_directory(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<PathBuf, WorkspaceError> {
        self.file_inspection().resolve_directory(path)
    }

    /// Streams an exact selection from this immutable snapshot into caller-owned storage.
    ///
    /// # Errors
    /// Rejects unsafe paths, absent selections, observed changes, and original I/O errors.
    pub fn copy_selection(
        &self,
        path: &WorkspacePath,
        selection: crate::FileReadSelection,
        output: &mut impl std::io::Write,
    ) -> Result<crate::InspectedSelection, WorkspaceError> {
        self.file_inspection().copy_selection(path, selection, output)
    }

    /// Captures selected snapshot bytes in owner-retained storage for resumable page reads.
    ///
    /// # Errors
    /// Rejects invalid/aliased storage, unsafe paths, absent ranges, changes, and I/O failures.
    pub fn capture_file(
        &self,
        path: &WorkspacePath,
        selection: crate::FileReadSelection,
        storage: fs::File,
    ) -> Result<crate::RetainedInspection, WorkspaceError> {
        self.file_inspection().capture_file(path, selection, storage)
    }

    /// Inspects one exact regular file or directory without following symlinks.
    ///
    /// # Errors
    /// Returns a typed failure for an absent, symlinked, special, or changed entry.
    pub fn metadata(&self, path: &WorkspacePath) -> Result<WorkspaceMetadata, WorkspaceError> {
        self.file_inspection().metadata(path)
    }

    /// Observes supported children together with exact native identities and typed exclusions.
    /// Protected metadata is hidden; unsupported children do not discard usable siblings.
    ///
    /// # Errors
    /// Rejects an unsafe directory, changed root/directory identity, or incomplete iteration.
    pub fn inspect_directory(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<crate::DirectoryListing, WorkspaceError> {
        self.file_inspection().inspect_directory(path)
    }

    /// Legacy projection of supported children in canonical authority-path order.
    /// Use `inspect_directory` for complete native-entry evidence and exclusions.
    ///
    /// # Errors
    /// Rejects an unsafe directory, changed root/directory identity, or incomplete iteration.
    pub fn list_directory(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<Vec<DirectoryEntry>, WorkspaceError> {
        let observed = self.inspect_directory(path)?;
        let mut entries = observed
            .items()
            .iter()
            .filter_map(|item| item.metadata().cloned().map(DirectoryEntry))
            .collect::<Vec<_>>();
        entries.sort_unstable_by(|left, right| left.0.path.cmp(&right.0.path));
        Ok(entries)
    }

    /// Captures exact native children and exclusions for owner-persisted resumable page reads.
    ///
    /// # Errors
    /// Rejects unsafe/changed directories, nonempty/aliased storage and I/O failures.
    pub fn capture_directory(
        &self,
        path: Option<&WorkspacePath>,
        storage: fs::File,
    ) -> Result<crate::RetainedDirectory, WorkspaceError> {
        self.file_inspection().capture_directory(path, storage)
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
        self.file_inspection()
            .read_file(path, crate::FileReadSelection::all(), maximum_bytes)
            .map(crate::InspectedFile::into_bytes)
    }
}

const fn directory_invalid(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::InvalidInput,
        WorkspaceOperation::Inspect,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

fn directory_io(error: std::io::Error) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Inspect,
        RecoveryClass::Reobserve,
        "selected workspace directory could not be canonicalized",
    )
    .with_io_source(error)
}
