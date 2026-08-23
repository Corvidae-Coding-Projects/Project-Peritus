//! Closed patch operation vocabulary.

use crate::{
    ErrorCode, FinalFile, PatchError, PatchOperationContext, Preimage, RecoveryClass,
    RollbackStatus, WorkspacePath,
};

/// Kind of one canonical patch operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatchOperationKind {
    /// Create a previously absent regular file.
    Create,
    /// Replace an exactly identified regular file.
    Replace,
    /// Delete an exactly identified regular file.
    Delete,
}

/// One checked create, replace, or delete operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchOperation {
    path: WorkspacePath,
    preimage: Preimage,
    final_file: Option<FinalFile>,
    kind: PatchOperationKind,
}

impl PatchOperation {
    /// Constructs a create operation whose target must be absent.
    #[must_use]
    pub const fn create(path: WorkspacePath, final_file: FinalFile) -> Self {
        Self {
            path,
            preimage: Preimage::Absent,
            final_file: Some(final_file),
            kind: PatchOperationKind::Create,
        }
    }

    /// Constructs a replacement with an exact present-file preimage.
    ///
    /// # Errors
    ///
    /// Returns invalid-content when `preimage` is absent.
    pub fn replace(
        path: WorkspacePath,
        preimage: Preimage,
        final_file: FinalFile,
    ) -> Result<Self, PatchError> {
        require_present(preimage)?;
        Ok(Self { path, preimage, final_file: Some(final_file), kind: PatchOperationKind::Replace })
    }

    /// Constructs a deletion with an exact present-file preimage.
    ///
    /// # Errors
    ///
    /// Returns invalid-content when `preimage` is absent.
    pub fn delete(path: WorkspacePath, preimage: Preimage) -> Result<Self, PatchError> {
        require_present(preimage)?;
        Ok(Self { path, preimage, final_file: None, kind: PatchOperationKind::Delete })
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

    /// Returns final file content for create and replace operations.
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
