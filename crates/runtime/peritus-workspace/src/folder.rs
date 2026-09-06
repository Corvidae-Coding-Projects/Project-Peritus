//! Canonical direct-directory identity, without Git discovery or recursive content inspection.

use peritus_types::Sha256Digest;
use std::path::{Path, PathBuf};

/// Observed directory identity used to bind in-place workspace trust.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderIdentity {
    root: PathBuf,
    digest: Sha256Digest,
}

impl FolderIdentity {
    /// Observes a canonical directory without executing tools, creating files, or reading children.
    ///
    /// # Errors
    /// Rejects unavailable, non-directory, and non-UTF-8 paths.
    pub fn observe(path: &Path) -> Result<Self, std::io::Error> {
        let root = path.canonicalize()?;
        let metadata = root.metadata()?;
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
        Ok(Self { root, digest: peritus_codec::sha256(&identity) })
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
}
