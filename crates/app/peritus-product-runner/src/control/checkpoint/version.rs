//! Exact checkpoint workspace-object states and legacy regular-file API compatibility.

use super::ControlError;
use peritus_patch::{DirectoryMode, FileMode, Preimage, SnapshotFile};
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

/// Complete retained before-image used after a command reveals its actual mutation footprint.
///
/// This capability is process-local and deliberately has no serialized representation. The
/// durable scope journal owns the declared identity and retained bytes; hosts must stream and
/// verify the snapshot before publishing the corresponding checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceMutationBaseline {
    version: CheckpointFileVersion,
    snapshot: Option<SnapshotFile>,
}

impl WorkspaceMutationBaseline {
    pub(crate) fn new(
        version: CheckpointFileVersion,
        snapshot: Option<SnapshotFile>,
    ) -> Result<Self, ControlError> {
        version.validate()?;
        let exact = match (version, snapshot.as_ref()) {
            (CheckpointVersion::Absent | CheckpointVersion::EmptyDirectory { .. }, None) => true,
            (
                CheckpointVersion::Present { digest, bytes, mode },
                Some(snapshot),
            ) => {
                snapshot.identity()
                    == Preimage::present(
                        Sha256Digest::new(digest),
                        bytes,
                        match mode {
                            CheckpointFileMode::Regular => FileMode::Regular,
                            CheckpointFileMode::Executable => FileMode::Executable,
                        },
                    )
            }
            _ => false,
        };
        if !exact {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { version, snapshot })
    }

    /// Returns the exact workspace state captured before the command.
    #[must_use]
    pub const fn version(&self) -> CheckpointFileVersion {
        self.version
    }

    /// Returns the verified streaming body for a present regular file.
    #[must_use]
    pub fn snapshot(&self) -> Option<&SnapshotFile> {
        self.snapshot.as_ref()
    }
}

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
