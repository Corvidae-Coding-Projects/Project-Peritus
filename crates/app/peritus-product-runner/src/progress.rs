//! Bounded candidate fingerprints used to replenish productive developer work segments.

use std::{
    fs::{self, File},
    io::Read as _,
    path::{Path, PathBuf},
};

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline, file_metadata,
};

/// Exact content and committed-HEAD identity of the current workspace state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceCheckpoint {
    head: String,
    entries: Vec<CheckpointEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckpointEntry {
    path: PathBuf,
    digest: Option<[u8; 32]>,
    permissions: Option<u32>,
    kind: ContentKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContentKind {
    File = 1,
    Symlink = 2,
    Repository = 3,
    Other = 4,
}

impl WorkspaceCheckpoint {
    pub(crate) fn scoped(root: &Path, paths: Vec<PathBuf>) -> Result<Self, ProductRunnerError> {
        let entries = paths
            .into_iter()
            .map(|path| checkpoint_entry(root, path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { head: "in-place-task-files-v1".to_owned(), entries })
    }

    /// Captures HEAD and the complete source snapshot, including nested repositories and modes.
    /// Git's changed-path list omits non-executable permission changes and may hide submodules.
    pub fn capture(root: &Path) -> Result<Self, ProductRunnerError> {
        let baseline = CandidateBaseline::capture(root)?;
        let entries = vec![CheckpointEntry {
            path: PathBuf::new(),
            digest: Some(crate::candidate::managed::ManagedBaseline::repository_fingerprint(root)?),
            permissions: None,
            kind: ContentKind::Repository,
        }];
        Ok(Self { head: baseline.head().to_owned(), entries })
    }

    /// Returns a canonical digest of HEAD, source contents, file kinds, and permissions.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hash_bytes(&mut hasher, self.head.as_bytes());
        for entry in &self.entries {
            hash_bytes(&mut hasher, entry.path.to_string_lossy().as_bytes());
            match entry.digest {
                Some(digest) => {
                    hasher.update([entry.kind as u8]);
                    hasher.update(digest);
                }
                None => hasher.update([0]),
            }
            match entry.permissions {
                Some(permissions) => {
                    hasher.update([1]);
                    hasher.update(permissions.to_le_bytes());
                }
                None => hasher.update([0]),
            }
        }
        Sha256Digest::new(hasher.finalize().into())
    }
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

fn checkpoint_entry(root: &Path, path: PathBuf) -> Result<CheckpointEntry, ProductRunnerError> {
    if !crate::candidate::managed::parent_is_directory(root, &path)? {
        return Ok(CheckpointEntry {
            path,
            digest: None,
            permissions: None,
            kind: ContentKind::Other,
        });
    }
    let absolute = root.join(&path);
    let (digest, permissions, kind) = match fs::symlink_metadata(&absolute) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (None, None, ContentKind::Other)
        }
        Err(error) => return Err(repository(error.to_string())),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = fs::read_link(&absolute).map_err(|error| repository(error.to_string()))?;
            (
                Some(Sha256::digest(target.as_os_str().as_encoded_bytes()).into()),
                Some(file_metadata::permission_fingerprint(&metadata)),
                ContentKind::Symlink,
            )
        }
        Ok(metadata) if metadata.is_file() => (
            Some(digest_file(&absolute)?),
            Some(file_metadata::permission_fingerprint(&metadata)),
            ContentKind::File,
        ),
        Ok(metadata) if metadata.is_dir() && absolute.join(".git").exists() => (
            Some(crate::candidate::managed::ManagedBaseline::repository_fingerprint(&absolute)?),
            Some(file_metadata::permission_fingerprint(&metadata)),
            ContentKind::Repository,
        ),
        Ok(metadata) => (
            Some(Sha256::digest(b"non-file").into()),
            Some(file_metadata::permission_fingerprint(&metadata)),
            ContentKind::Other,
        ),
    };
    Ok(CheckpointEntry { path, digest, permissions, kind })
}

fn digest_file(path: &Path) -> Result<[u8; 32], ProductRunnerError> {
    let mut file = File::open(path).map_err(|error| repository(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| repository(error.to_string()))?;
        if count == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&buffer[..count]);
    }
}

fn repository(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Repository,
        "checkpoint developer progress",
        detail,
    )
}

#[cfg(test)]
mod tests;
