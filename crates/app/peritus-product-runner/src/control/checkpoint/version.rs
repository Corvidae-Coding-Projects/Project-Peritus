//! Exact checkpoint workspace-object states and legacy regular-file API compatibility.

use super::ControlError;
use peritus_patch::DirectoryMode;
use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};

/// Portable file mode bound into checkpoint preconditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointFileMode {
    /// Regular non-executable file.
    Regular,
    /// Regular executable file.
    Executable,
}

/// Exact expected workspace-object state. Absence and directories are represented explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum CheckpointVersion {
    /// The covered path did not exist.
    Absent,
    /// An empty directory with exact portable permission intent.
    EmptyDirectory {
        /// Unix permission and special bits; non-Unix hosts use representable intents.
        permissions: u16,
    },
    /// The covered path was a regular file with exact bytes and portable mode.
    Present {
        /// SHA-256 of the complete file.
        digest: [u8; 32],
        /// Complete byte length.
        bytes: u64,
        /// Portable mode checked by the restore backend.
        mode: CheckpointFileMode,
    },
}
/// Compatibility name retained for existing file checkpoint callers and stored records.
pub type CheckpointFileVersion = CheckpointVersion;

impl CheckpointVersion {
    /// Constructs an exact empty-directory state from checked permission intent.
    #[must_use]
    pub const fn empty_directory(mode: DirectoryMode) -> Self {
        Self::EmptyDirectory { permissions: mode.bits() }
    }
    /// Constructs a present-file version from exact content facts.
    #[must_use]
    pub const fn present(digest: Sha256Digest, bytes: u64, mode: CheckpointFileMode) -> Self {
        Self::Present { digest: digest.into_bytes(), bytes, mode }
    }
    /// Returns the content digest when this version is present.
    #[must_use]
    pub const fn digest(self) -> Option<Sha256Digest> {
        match self {
            Self::Absent | Self::EmptyDirectory { .. } => None,
            Self::Present { digest, .. } => Some(Sha256Digest::new(digest)),
        }
    }
    /// Returns the complete byte length when present.
    #[must_use]
    pub const fn bytes(self) -> Option<u64> {
        match self {
            Self::Absent | Self::EmptyDirectory { .. } => None,
            Self::Present { bytes, .. } => Some(bytes),
        }
    }
    /// Returns the portable mode when present.
    #[must_use]
    pub const fn mode(self) -> Option<CheckpointFileMode> {
        match self {
            Self::Absent | Self::EmptyDirectory { .. } => None,
            Self::Present { mode, .. } => Some(mode),
        }
    }

    pub(super) fn validate(self) -> Result<(), ControlError> {
        if let Self::EmptyDirectory { permissions } = self {
            DirectoryMode::new(permissions).map_err(|_| ControlError::InvalidInput)?;
        }
        Ok(())
    }
}
