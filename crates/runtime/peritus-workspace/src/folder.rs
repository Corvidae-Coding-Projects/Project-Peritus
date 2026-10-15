//! Canonical direct-directory identity, without Git discovery or recursive content inspection.

use peritus_types::Sha256Digest;
use std::path::{Path, PathBuf};

/// Observed directory identity used to bind in-place workspace trust.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderIdentity {
    root: PathBuf,
    digest: Sha256Digest,
    legacy_digest: Sha256Digest,
}

impl FolderIdentity {
    /// Observes a canonical directory without executing tools, creating files, or reading children.
    ///
    /// # Errors
    /// Rejects unavailable, non-directory, and non-UTF-8 paths.
    pub fn observe(path: &Path) -> Result<Self, std::io::Error> {
        let root = path.canonicalize()?;
        let metadata = root.metadata()?;
        Self::from_metadata(root, &metadata)
    }

    pub(crate) fn from_metadata(
        root: PathBuf,
        metadata: &std::fs::Metadata,
    ) -> Result<Self, std::io::Error> {
        if !metadata.is_dir() {
            return Err(std::io::Error::other("workspace path must be a directory"));
        }
        let text =
            root.to_str().ok_or_else(|| std::io::Error::other("workspace path is not UTF-8"))?;
        let mut identity = b"peritus-direct-folder-v1\0".to_vec();
        identity.extend_from_slice(text.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            identity.extend_from_slice(&metadata.dev().to_le_bytes());
            identity.extend_from_slice(&metadata.ino().to_le_bytes());
        }
        #[cfg(not(unix))]
        {
            let created = metadata
                .created()?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)?;
            identity.extend_from_slice(&created.as_nanos().to_le_bytes());
        }
        let legacy_digest = peritus_codec::sha256(&identity);
        // Linux can assign a different anonymous device number to the same Btrfs
        // filesystem after a reboot. Directory birth time and inode survive it,
        // while replacing the directory changes its identity. Filesystems without
        // birth times retain the original device/inode identity and repair path.
        #[cfg(unix)]
        if let Ok(created) = metadata.created() {
            use std::os::unix::fs::MetadataExt as _;
            let created =
                created.duration_since(std::time::UNIX_EPOCH).map_err(std::io::Error::other)?;
            let mut stable = b"peritus-direct-folder-v2\0".to_vec();
            stable.extend_from_slice(text.as_bytes());
            stable.extend_from_slice(&metadata.ino().to_le_bytes());
            stable.extend_from_slice(&created.as_nanos().to_le_bytes());
            return Ok(Self { root, digest: peritus_codec::sha256(&stable), legacy_digest });
        }
        Ok(Self { root, digest: legacy_digest, legacy_digest })
    }

    /// Borrows the canonical folder selected by the user.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns a domain-separated identity; it is not a content snapshot or Git identity.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Accepts the current identity or a legacy device/inode identity on this mount.
    #[must_use]
    pub fn matches_digest(&self, digest: Sha256Digest) -> bool {
        digest == self.digest || digest == self.legacy_digest
    }

    /// Original device/inode identity, for authenticated migration of existing trust.
    #[must_use]
    pub const fn legacy_digest(&self) -> Sha256Digest {
        self.legacy_digest
    }
}
