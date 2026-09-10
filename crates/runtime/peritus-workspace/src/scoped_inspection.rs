//! Handle-relative, bounded reads for explicit live-folder references, not mutation authority.

use crate::{ErrorCode, FolderIdentity, RecoveryClass, WorkspaceError, WorkspaceOperation};
use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _, OpenOptionsSyncExt as _};
use cap_std::fs::{Dir, File, OpenOptions};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use std::io;

mod read;
mod selection;
#[cfg(test)]
mod tests;
pub use selection::FileReadSelection;

/// Maximum source file scanned to establish a complete digest for an explicit range.
pub const MAX_INSPECTION_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// A root-bound read-only capability for explicitly selected relative files.
///
/// The host must first authorize the actor, selected workspace, and protected-path policy.
/// This type does not grant permissions, invoke tools, or create a workspace snapshot. Each
/// path component is opened relative to its parent handle without following symlinks.
pub struct FolderInspection {
    identity: FolderIdentity,
    root: Dir,
}

/// Exact selected bytes and complete source digest from one bounded, checked read.
///
/// This is an observation, not a lock against subsequent edits or proof that an arbitrarily
/// concurrent writer supplied an atomic version. The host publishes these bytes as immutable
/// artifacts and binds any future refresh to its own revisioned admission transaction.
#[derive(Clone)]
pub struct InspectedFile {
    path: WorkspacePath,
    source_bytes: u64,
    source_digest: Sha256Digest,
    range: (u64, u64),
    bytes: Vec<u8>,
}
impl InspectedFile {
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    /// Borrows the exact canonical workspace-relative source path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }
    /// Returns complete source size, including bytes outside the selected range.
    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    /// Returns SHA-256 of the complete scanned source.
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest {
        self.source_digest
    }
    /// Returns the resolved half-open byte range in the complete source.
    #[must_use]
    pub const fn range(&self) -> (u64, u64) {
        self.range
    }
    /// Borrows exactly the selected bytes; no silent truncation occurs.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns SHA-256 of the selected bytes.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        peritus_codec::sha256(&self.bytes)
    }
}
impl std::fmt::Debug for InspectedFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InspectedFile")
            .field("source_bytes", &self.source_bytes)
            .field("range", &self.range)
            .finish_non_exhaustive()
    }
}

impl FolderInspection {
    /// Opens the exact observed folder and verifies identity using the opened root handle.
    ///
    /// # Errors
    /// Rejects root replacement, reparse points, non-directories, and unavailable roots.
    pub fn open(identity: &FolderIdentity) -> Result<Self, WorkspaceError> {
        let directory = Dir::open_ambient_dir(identity.root(), cap_std::ambient_authority())
            .map_err(|error| read_error(&error))?;
        let file = directory.into_std_file();
        let metadata = file.metadata().map_err(|error| read_error(&error))?;
        reject_reparse(&metadata)?;
        let observed = FolderIdentity::from_metadata(identity.root().to_path_buf(), &metadata)
            .map_err(|error| read_error(&error))?;
        if &observed != identity {
            return Err(changed());
        }
        Ok(Self { identity: identity.clone(), root: Dir::from_std_file(file) })
    }

    /// Borrows the exact root identity used to open this read-only capability.
    #[must_use]
    pub const fn identity(&self) -> &FolderIdentity {
        &self.identity
    }

    fn open_file(&self, path: &WorkspacePath) -> Result<File, WorkspaceError> {
        let mut components = path.as_str().split('/').peekable();
        let mut parent = self.root.try_clone().map_err(|error| read_error(&error))?;
        while let Some(component) = components.next() {
            if components.peek().is_none() {
                let mut options = OpenOptions::new();
                options.read(true).follow(FollowSymlinks::No).nonblock(true);
                let file =
                    parent.open_with(component, &options).map_err(|error| read_error(&error))?;
                let metadata = file.metadata().map_err(|error| read_error(&error))?;
                reject_cap_reparse(&metadata)?;
                if !metadata.is_file() || metadata.is_symlink() {
                    return Err(invalid("selected source is not a regular file"));
                }
                return Ok(file);
            }
            parent = parent.open_dir_nofollow(component).map_err(|error| read_error(&error))?;
            reject_cap_reparse(&parent.dir_metadata().map_err(|error| read_error(&error))?)?;
        }
        Err(invalid("source path is empty"))
    }
}

fn reject_reparse(metadata: &std::fs::Metadata) -> Result<(), WorkspaceError> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("workspace root is a reparse point"));
        }
    }
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("workspace root is not a directory"));
    }
    Ok(())
}
fn reject_cap_reparse(metadata: &cap_std::fs::Metadata) -> Result<(), WorkspaceError> {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt as _;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("source path contains a reparse point"));
        }
    }
    if metadata.is_symlink() {
        return Err(invalid("source path contains a symbolic link"));
    }
    Ok(())
}
fn read_error(error: &io::Error) -> WorkspaceError {
    let (code, recovery) =
        if matches!(error.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::PermissionDenied) {
            (ErrorCode::InvalidInput, RecoveryClass::CorrectRequest)
        } else {
            (ErrorCode::Indeterminate, RecoveryClass::Reobserve)
        };
    WorkspaceError::new(
        code,
        WorkspaceOperation::Inspect,
        recovery,
        "scoped file inspection failed; no content was published",
    )
}
const fn invalid(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::InvalidInput,
        WorkspaceOperation::Inspect,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
const fn changed() -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Inspect,
        RecoveryClass::Reobserve,
        "source identity or content changed during inspection",
    )
}
