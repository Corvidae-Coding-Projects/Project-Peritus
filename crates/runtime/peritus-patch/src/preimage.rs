//! Exact file preimages and portable mode intent.

use peritus_types::Sha256Digest;

use crate::{ErrorCode, PatchError, PatchOperationContext, RecoveryClass, RollbackStatus};

/// Exact directory permission intent, separate from regular-file executable mode.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DirectoryMode(u16);

impl DirectoryMode {
    /// Checks Unix permission and special bits without admitting a filesystem node type.
    /// Non-Unix adapters support only their explicitly representable permission intents.
    ///
    /// # Errors
    /// Rejects bits outside the directory permission vocabulary.
    pub const fn new(bits: u16) -> Result<Self, PatchError> {
        if bits & !0o7777 != 0 {
            return Err(PatchError::message(
                ErrorCode::InvalidContent,
                RecoveryClass::CorrectPatch,
                PatchOperationContext::Plan,
                RollbackStatus::NotRequired,
                "directory permission intent contains unsupported bits",
            ));
        }
        Ok(Self(bits))
    }

    /// Returns exact permission bits.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

/// Portable regular-file mode represented by patches.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FileMode {
    /// A non-executable regular file (`100644` in Git).
    Regular,
    /// An executable regular file (`100755` in Git); filesystem application requires Unix mode bits.
    Executable,
}

impl FileMode {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Regular => 1,
            Self::Executable => 2,
        }
    }

    pub(crate) const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Regular),
            2 => Some(Self::Executable),
            _ => None,
        }
    }
}

/// Exact expected object type and state of a patch target before mutation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Preimage {
    /// The target must not exist.
    Absent,
    /// The target must be an empty directory with exact permission intent.
    EmptyDirectory {
        /// Directory permissions; this is never a regular-file executable mode.
        mode: DirectoryMode,
    },
    /// The target must be a regular file with exact content identity and mode.
    Present {
        /// SHA-256 of the exact file bytes.
        digest: Sha256Digest,
        /// Exact byte length.
        size: u64,
        /// Portable regular/executable mode.
        mode: FileMode,
    },
}

impl Preimage {
    /// Creates a present-file preimage from exact expected values.
    #[must_use]
    pub const fn present(digest: Sha256Digest, size: u64, mode: FileMode) -> Self {
        Self::Present { digest, size, mode }
    }

    /// Computes a present-file preimage from exact bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8], mode: FileMode) -> Self {
        Self::Present {
            digest: peritus_codec::sha256(bytes),
            size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            mode,
        }
    }
}
