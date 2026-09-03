//! Exact read-only candidate capture at the external adapter boundary.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::BenchmarkError;

/// Stable digest and paths for the exact workspace candidate observed at settlement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateSnapshot {
    /// SHA-256 over the ordered path and content observations.
    pub digest: String,
    /// Raw digest bytes used by the verified settlement identity.
    #[serde(skip)]
    pub digest_bytes: [u8; 32],
    /// Sorted paths changed relative to the admitted Git baseline.
    pub changed_paths: Vec<PathBuf>,
}

/// Captures committed, staged, unstaged, deleted, and untracked paths without modifying state.
pub fn capture(
    root: &Path,
    baseline_revision: Option<&str>,
) -> Result<CandidateSnapshot, BenchmarkError> {
    let mut changed_paths = if let Some(baseline) = baseline_revision {
        git_paths(
            root,
            "enumerate candidate paths from baseline",
            &["diff", "--name-only", "-z", baseline, "--"],
        )?
    } else {
        git_paths(
            root,
            "enumerate modified candidate paths",
            &["ls-files", "--modified", "--deleted", "-z"],
        )?
    };
    changed_paths.extend(git_paths(
        root,
        "enumerate untracked candidate paths",
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    changed_paths.sort();
    changed_paths.dedup();

    let mut hasher = Sha256::new();
    hasher.update(b"peritus.external-candidate.v1\0");
    for relative in &changed_paths {
        let portable = relative.to_string_lossy();
        hasher.update(portable.as_bytes());
        hasher.update(b"\0");
        hash_path(root, relative, &mut hasher)?;
        hasher.update(b"\0");
    }
    let digest_bytes: [u8; 32] = hasher.finalize().into();
    Ok(CandidateSnapshot { digest: lowercase_hex(&digest_bytes), digest_bytes, changed_paths })
}

fn git_paths(
    root: &Path,
    operation: &'static str,
    arguments: &[&str],
) -> Result<Vec<PathBuf>, BenchmarkError> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| BenchmarkError::filesystem(operation, root, error))?;
    if !output.status.success() {
        return Err(BenchmarkError::Command {
            operation,
            status: output.status.to_string(),
            detail: bounded(&output.stderr),
        });
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(path_from_git)
        .collect())
}

fn hash_path(root: &Path, relative: &Path, hasher: &mut Sha256) -> Result<(), BenchmarkError> {
    let absolute = root.join(relative);
    match fs::symlink_metadata(&absolute) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            hasher.update(b"symlink\0");
            let target = fs::read_link(&absolute).map_err(|error| {
                BenchmarkError::filesystem("read candidate symlink", &absolute, error)
            })?;
            hasher.update(target.to_string_lossy().as_bytes());
        }
        Ok(metadata) if metadata.is_file() => {
            hasher.update(b"file\0");
            hash_permissions(&metadata, hasher);
            let bytes = fs::read(&absolute).map_err(|error| {
                BenchmarkError::filesystem("read candidate file", &absolute, error)
            })?;
            hasher.update(bytes);
        }
        Ok(_) => hasher.update(b"other"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            hasher.update(b"deleted");
        }
        Err(error) => {
            return Err(BenchmarkError::filesystem("inspect candidate path", &absolute, error));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn hash_permissions(metadata: &fs::Metadata, hasher: &mut Sha256) {
    use std::os::unix::fs::PermissionsExt as _;
    hasher.update((metadata.permissions().mode() & 0o7777).to_le_bytes());
}

#[cfg(not(unix))]
fn hash_permissions(metadata: &fs::Metadata, hasher: &mut Sha256) {
    hasher.update([u8::from(metadata.permissions().readonly())]);
}

#[cfg(unix)]
fn path_from_git(bytes: &[u8]) -> PathBuf {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _};
    PathBuf::from(OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_from_git(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn bounded(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(64 * 1024)]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_digest_changes_with_content_and_tracks_deletion() {
        let root = tempfile::tempdir().expect("workspace");
        fs::write(root.path().join("tracked.txt"), "before\n").expect("fixture");
        let baseline = crate::workspace::prepare(root.path()).expect("baseline");
        fs::write(root.path().join("tracked.txt"), "after\n").expect("change");
        fs::write(root.path().join("new.txt"), "new\n").expect("untracked");

        let first = capture(root.path(), Some(&baseline.head)).expect("first candidate");
        assert!(
            Command::new("git")
                .args(["add", "--all", "."])
                .current_dir(root.path())
                .status()
                .expect("stage candidate")
                .success()
        );
        assert!(
            Command::new("git")
                .args([
                    "-c",
                    "user.name=Peritus Test",
                    "-c",
                    "user.email=peritus-test@localhost",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "--quiet",
                    "-m",
                    "Commit candidate",
                ])
                .current_dir(root.path())
                .status()
                .expect("commit candidate")
                .success()
        );
        let committed = capture(root.path(), Some(&baseline.head)).expect("committed candidate");
        fs::remove_file(root.path().join("tracked.txt")).expect("delete");
        let second = capture(root.path(), Some(&baseline.head)).expect("second candidate");

        assert_eq!(
            first.changed_paths,
            vec![PathBuf::from("new.txt"), PathBuf::from("tracked.txt")]
        );
        assert_eq!(committed, first);
        assert_eq!(second.changed_paths, first.changed_paths);
        assert_ne!(first.digest, second.digest);
    }
}
