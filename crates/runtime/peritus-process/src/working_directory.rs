//! Checked working-directory identity.

use std::{
    fs,
    path::{Path, PathBuf},
};

use peritus_types::{EnvironmentId, Generation, ResourceId, RevisionNumber, WorkspaceId};

use crate::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

/// Whether a process may mutate its workspace target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkspaceAccess {
    /// The process target is immutable and must not carry mutation-lease authority.
    ReadOnly,
    /// The process may mutate the exact generation/revision under a committed lease use.
    Writable,
}

/// Exact nominal and physical binding of one process working directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingDirectory {
    canonical_path: PathBuf,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    generation: Generation,
    revision: RevisionNumber,
    access: WorkspaceAccess,
}

impl WorkingDirectory {
    pub(crate) fn restore_canonical_path(
        canonical_path: PathBuf,
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        generation: Generation,
        revision: RevisionNumber,
        access: WorkspaceAccess,
    ) -> Result<Self, ProcessError> {
        if canonical_path.as_os_str().is_empty()
            || !canonical_path.is_absolute()
            || crate::command::contains_nul(canonical_path.as_os_str())
        {
            return Err(cwd_error("restored canonical working directory is invalid"));
        }
        Ok(Self {
            canonical_path,
            workspace_id,
            resource_id,
            environment_id,
            generation,
            revision,
            access,
        })
    }

    /// Opens and canonicalizes a directory before authorization.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is missing, not a directory, or cannot be canonicalized.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        path: impl AsRef<Path>,
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        generation: Generation,
        revision: RevisionNumber,
        access: WorkspaceAccess,
    ) -> Result<Self, ProcessError> {
        let metadata = fs::metadata(path.as_ref())
            .map_err(|_| cwd_error("working directory cannot be inspected"))?;
        if !metadata.is_dir() {
            return Err(cwd_error("working directory is not a directory"));
        }
        let canonical_path = fs::canonicalize(path.as_ref())
            .map_err(|_| cwd_error("working directory cannot be canonicalized"))?;
        Ok(Self {
            canonical_path,
            workspace_id,
            resource_id,
            environment_id,
            generation,
            revision,
            access,
        })
    }

    /// Returns the canonical host path fixed before authorization.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.canonical_path
    }

    /// Returns the exact workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Returns the exact target resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Returns the exact execution environment.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }

    /// Returns the mutation-fencing generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Returns the immutable workspace revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }

    /// Returns the requested workspace access.
    #[must_use]
    pub const fn access(&self) -> WorkspaceAccess {
        self.access
    }

    pub(crate) fn uses_legacy_encoding(&self) -> bool {
        self.canonical_path.to_str().is_some()
    }
}

const fn cwd_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidWorkingDirectory,
        ProcessOperation::Validate,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
