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
}

impl WorkspaceCheckpoint {
    /// Captures HEAD and streams every changed file into a digest without retaining its contents.
    pub fn capture(root: &Path) -> Result<Self, ProductRunnerError> {
        let baseline = CandidateBaseline::capture(root)?;
        let entries = baseline
            .changed_paths(root)?
            .into_iter()
            .map(|path| checkpoint_entry(root, path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { head: baseline.head().to_owned(), entries })
    }

    /// Returns a canonical digest of HEAD, every changed path, its content kind, and permissions.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hash_bytes(&mut hasher, self.head.as_bytes());
        for entry in &self.entries {
            hash_bytes(&mut hasher, entry.path.to_string_lossy().as_bytes());
            match entry.digest {
                Some(digest) => {
                    hasher.update([1]);
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
    let absolute = root.join(&path);
    let (digest, permissions) = match fs::symlink_metadata(&absolute) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, None),
        Err(error) => return Err(repository(error.to_string())),
        Ok(metadata) if metadata.is_file() => {
            (Some(digest_file(&absolute)?), Some(file_metadata::permission_fingerprint(&metadata)))
        }
        Ok(metadata) => (
            Some(Sha256::digest(b"non-file").into()),
            Some(file_metadata::permission_fingerprint(&metadata)),
        ),
    };
    Ok(CheckpointEntry { path, digest, permissions })
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
mod tests {
    use std::{fs, process::Command};

    use super::*;

    #[test]
    fn checkpoint_changes_only_when_candidate_content_changes() {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        fs::write(root.path().join("tracked.txt"), "baseline").expect("write baseline");
        run(root.path(), &["add", "."]);
        run(root.path(), &["commit", "--quiet", "-m", "fixture"]);

        let clean = WorkspaceCheckpoint::capture(root.path()).expect("clean");
        let same = WorkspaceCheckpoint::capture(root.path()).expect("same");
        assert_eq!(clean, same);

        fs::write(root.path().join("tracked.txt"), "changed").expect("write change");
        let changed = WorkspaceCheckpoint::capture(root.path()).expect("changed");
        assert_ne!(clean, changed);

        fs::write(root.path().join("new.txt"), "untracked").expect("write untracked");
        let untracked = WorkspaceCheckpoint::capture(root.path()).expect("untracked");
        assert_ne!(changed, untracked);
    }

    #[test]
    fn checkpoint_changes_when_the_task_creates_a_commit() {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        run(root.path(), &["commit", "--quiet", "--allow-empty", "-m", "fixture"]);
        let before = WorkspaceCheckpoint::capture(root.path()).expect("before commit");

        run(root.path(), &["commit", "--quiet", "--allow-empty", "-m", "task effect"]);
        let after = WorkspaceCheckpoint::capture(root.path()).expect("after commit");

        assert_ne!(before, after);
    }

    #[cfg(unix)]
    #[test]
    fn checkpoint_changes_when_candidate_permissions_change() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        fs::write(root.path().join("baseline.txt"), "baseline").expect("write baseline");
        run(root.path(), &["add", "."]);
        run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
        let candidate = root.path().join("private.key");
        fs::write(&candidate, "secret").expect("write candidate");
        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o644))
            .expect("initial permissions");
        let before = WorkspaceCheckpoint::capture(root.path()).expect("before permissions");

        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o600))
            .expect("fixed permissions");
        let after = WorkspaceCheckpoint::capture(root.path()).expect("after permissions");

        assert_ne!(before, after);
    }

    fn run(root: &Path, arguments: &[&str]) {
        assert!(
            Command::new("git").args(arguments).current_dir(root).status().expect("git").success()
        );
    }
}
