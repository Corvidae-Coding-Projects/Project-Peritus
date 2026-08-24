//! Exact file preimages and portable mode intent.

use peritus_types::Sha256Digest;

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

/// Exact expected state of a patch target before mutation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Preimage {
    /// The target must not exist.
    Absent,
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
