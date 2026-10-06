//! Closed patch operation vocabulary.

use crate::{
    DirectoryMode, ErrorCode, FinalFile, PatchError, PatchOperationContext, Preimage,
    RecoveryClass, RollbackStatus, SnapshotFile, WorkspacePath,
};

/// Kind of one canonical patch operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatchOperationKind {
    /// Create a previously absent regular file.
    Create,
    /// Replace an exactly identified regular file or empty directory.
    Replace,
    /// Delete an exactly identified regular file.
    Delete,
    /// Create a previously absent empty directory.
    CreateDirectory,
    /// Remove an exactly identified empty directory without recursive deletion.
    DeleteDirectory,
}

/// One checked regular-file or empty-directory operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchOperation {
    path: WorkspacePath,
    preimage: Preimage,
    final_file: Option<FinalFile>,
    snapshot: Option<SnapshotFile>,
    kind: PatchOperationKind,
    directory: Option<DirectoryMode>,
}

impl PatchOperation {
    /// Constructs a create operation whose target must be absent.
    #[must_use]
    pub const fn create(path: WorkspacePath, final_file: FinalFile) -> Self {
        Self {
            path,
            preimage: Preimage::Absent,
            final_file: Some(final_file),
            snapshot: None,
            kind: PatchOperationKind::Create,
            directory: None,
        }
    }

    /// Constructs a regular-file replacement with an exact existing-node preimage.
    ///
    /// # Errors
    ///
    /// Returns invalid-content when `preimage` is absent.
    pub fn replace(
        path: WorkspacePath,
        preimage: Preimage,
        final_file: FinalFile,
    ) -> Result<Self, PatchError> {
        require_existing(preimage)?;
        Ok(Self {
            path,
            preimage,
            final_file: Some(final_file),
            snapshot: None,
            kind: PatchOperationKind::Replace,
            directory: None,
        })
    }

    /// Constructs a deletion with an exact present-file preimage.
    ///
    /// # Errors
    ///
    /// Returns invalid-content when `preimage` is absent.
    pub fn delete(path: WorkspacePath, preimage: Preimage) -> Result<Self, PatchError> {
        require_present(preimage)?;
        Ok(Self {
            path,
            preimage,
            final_file: None,
            snapshot: None,
            kind: PatchOperationKind::Delete,
            directory: None,
        })
    }

    /// Creates an absent target from an owned streaming snapshot.
    #[must_use]
    pub const fn create_snapshot(path: WorkspacePath, snapshot: SnapshotFile) -> Self {
        Self {
            path,
            preimage: Preimage::Absent,
            final_file: None,
            snapshot: Some(snapshot),
            kind: PatchOperationKind::Create,
            directory: None,
        }
    }

    /// Replaces an exactly identified target from an owned streaming snapshot.
    ///
    /// # Errors
    /// Rejects an absent preimage.
    pub fn replace_snapshot(
        path: WorkspacePath,
        preimage: Preimage,
        snapshot: SnapshotFile,
    ) -> Result<Self, PatchError> {
        require_existing(preimage)?;
        Ok(Self {
            path,
            preimage,
            final_file: None,
            snapshot: Some(snapshot),
            kind: PatchOperationKind::Replace,
            directory: None,
        })
    }

    /// Creates an empty directory whose target must be absent.
    #[must_use]
    pub const fn create_directory(path: WorkspacePath, mode: DirectoryMode) -> Self {
        Self {
            path,
            preimage: Preimage::Absent,
            final_file: None,
            snapshot: None,
            kind: PatchOperationKind::CreateDirectory,
            directory: Some(mode),
        }
    }

    /// Restores empty-directory type and permissions over an exactly identified existing node.
    ///
    /// # Errors
    /// Rejects an absent preimage; creation has its own explicit operation.
    pub fn replace_directory(
        path: WorkspacePath,
        preimage: Preimage,
        mode: DirectoryMode,
    ) -> Result<Self, PatchError> {
        require_existing(preimage)?;
        Ok(Self {
            path,
            preimage,
            final_file: None,
            snapshot: None,
            kind: PatchOperationKind::Replace,
            directory: Some(mode),
        })
    }

    /// Removes an empty directory with an exact permission preimage.
    #[must_use]
    pub const fn delete_directory(path: WorkspacePath, mode: DirectoryMode) -> Self {
        Self {
            path,
            preimage: Preimage::EmptyDirectory { mode },
            final_file: None,
            snapshot: None,
            kind: PatchOperationKind::DeleteDirectory,
            directory: None,
        }
    }

    /// Returns the exact postimage, regardless of whether content is inline or streamed.
    #[must_use]
    pub fn postimage(&self) -> Preimage {
        if let Some(mode) = self.directory {
            return Preimage::EmptyDirectory { mode };
        }
        if let Some(snapshot) = &self.snapshot {
            return snapshot.identity();
        }
        self.final_file.as_ref().map_or(Preimage::Absent, |file| {
            Preimage::present(file.digest(), file.size(), file.mode())
        })
    }

    pub(crate) const fn is_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }

    pub(crate) const fn covers_directory(&self) -> bool {
        matches!(self.preimage, Preimage::EmptyDirectory { .. }) || self.directory.is_some()
    }

    pub(crate) fn write_final_to(&self, output: &mut dyn std::io::Write) -> Result<(), PatchError> {
        if let Some(snapshot) = &self.snapshot {
            return snapshot.write_to(output);
        }
        if let Some(file) = &self.final_file {
            output.write_all(file.bytes()).map_err(|error| {
                PatchError::io(
                    PatchOperationContext::StageFinal,
                    RollbackStatus::NotRequired,
                    error,
                )
            })?;
        }
        Ok(())
    }

    /// Returns the checked target path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }

    /// Returns the operation kind.
    #[must_use]
    pub const fn kind(&self) -> PatchOperationKind {
        self.kind
    }

    /// Returns the exact required preimage.
    #[must_use]
    pub const fn preimage(&self) -> Preimage {
        self.preimage
    }

    /// Returns inline final content. Streamed operations expose their identity via `postimage`.
    #[must_use]
    pub const fn final_file(&self) -> Option<&FinalFile> {
        self.final_file.as_ref()
    }
}

const fn require_present(preimage: Preimage) -> Result<(), PatchError> {
    if matches!(preimage, Preimage::Present { .. }) {
        Ok(())
    } else {
        Err(PatchError::message(
            ErrorCode::InvalidContent,
            RecoveryClass::CorrectPatch,
            PatchOperationContext::Plan,
            RollbackStatus::NotRequired,
            "replace and delete operations require a present preimage",
        ))
    }
}

const fn require_existing(preimage: Preimage) -> Result<(), PatchError> {
    if matches!(preimage, Preimage::Absent) { require_present(preimage) } else { Ok(()) }
}
