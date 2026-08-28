//! Bounded candidate fingerprints used to replenish productive developer work segments.

use std::{
    fs::{self, File},
    io::Read as _,
    path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};

use crate::{ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline};

/// Exact content identity of the candidate relative to its stable Git HEAD.
#[derive(Eq, PartialEq)]
pub struct WorkspaceCheckpoint {
    entries: Vec<CheckpointEntry>,
}

#[derive(Eq, PartialEq)]
struct CheckpointEntry {
    path: PathBuf,
    digest: Option<[u8; 32]>,
}

impl WorkspaceCheckpoint {
    /// Streams every changed file into a stable digest without retaining its contents in memory.
    pub fn capture(root: &Path) -> Result<Self, ProductRunnerError> {
        let baseline = CandidateBaseline::capture(root)?;
        let entries = baseline
            .changed_paths(root)?
            .into_iter()
            .map(|path| checkpoint_entry(root, path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { entries })
    }
}

fn checkpoint_entry(root: &Path, path: PathBuf) -> Result<CheckpointEntry, ProductRunnerError> {
    let absolute = root.join(&path);
    let digest = match fs::symlink_metadata(&absolute) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(repository(error.to_string())),
        Ok(metadata) if metadata.is_file() => Some(digest_file(&absolute)?),
        Ok(_) => Some(Sha256::digest(b"non-file").into()),
    };
    Ok(CheckpointEntry { path, digest })
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
        assert!(clean == same);

        fs::write(root.path().join("tracked.txt"), "changed").expect("write change");
        let changed = WorkspaceCheckpoint::capture(root.path()).expect("changed");
        assert!(clean != changed);

        fs::write(root.path().join("new.txt"), "untracked").expect("write untracked");
        let untracked = WorkspaceCheckpoint::capture(root.path()).expect("untracked");
        assert!(changed != untracked);
    }

    fn run(root: &Path, arguments: &[&str]) {
        assert!(
            Command::new("git").args(arguments).current_dir(root).status().expect("git").success()
        );
    }
}
