//! Exact committed-source observations shared by H0 preparation and execution.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

/// Failure to inspect the exact candidate repository.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// The path was not a canonical Peritus repository or its state was not admissible.
    #[error("H0 candidate repository: {0}")]
    Invalid(&'static str),
    /// A filesystem or process operation failed.
    #[error("H0 candidate repository {operation} `{path}`: {source}")]
    Io {
        operation: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A Git observation returned unsuccessfully.
    #[error("H0 candidate repository could not {operation}: {status}")]
    Command { operation: &'static str, status: ExitStatus },
    /// Git returned text outside the reviewed UTF-8 protocol.
    #[error("H0 candidate repository returned non-UTF-8 {0}")]
    Encoding(&'static str),
}

/// Canonical handle to one committed Peritus candidate checkout.
pub struct CandidateRepository {
    root: PathBuf,
}

impl CandidateRepository {
    pub(super) fn open(path: &Path) -> Result<Self, RepositoryError> {
        let root = fs::canonicalize(path).map_err(|source| RepositoryError::Io {
            operation: "canonicalize",
            path: path.display().to_string(),
            source,
        })?;
        if !root.join("Cargo.toml").is_file() || !root.join("architecture.toml").is_file() {
            return Err(RepositoryError::Invalid("candidate root is not a Peritus workspace"));
        }
        Ok(Self { root })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn into_root(self) -> PathBuf {
        self.root
    }

    pub(super) fn verify_clean(&self) -> Result<(), RepositoryError> {
        let output = self.git_output(&["status", "--porcelain=v1", "--untracked-files=all"])?;
        if !output.status.success() {
            return Err(RepositoryError::Command {
                operation: "inspect exact candidate checkout",
                status: output.status,
            });
        }
        if !output.stdout.is_empty() {
            return Err(RepositoryError::Invalid(
                "candidate checkout differs from its committed source identity",
            ));
        }
        Ok(())
    }

    pub(super) fn source_digest(&self) -> Result<Sha256Digest, RepositoryError> {
        self.archive_digest(&[])
    }

    pub(super) fn archive_digest(&self, paths: &[&str]) -> Result<Sha256Digest, RepositoryError> {
        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .args(["archive", "--format=tar", "HEAD"])
            .args(paths)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(|source| RepositoryError::Io {
            operation: "start exact source archive",
            path: self.root.display().to_string(),
            source,
        })?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or(RepositoryError::Invalid("git archive stdout is unavailable"))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
        loop {
            let count = stdout.read(&mut buffer).map_err(|source| RepositoryError::Io {
                operation: "read exact source archive",
                path: self.root.display().to_string(),
                source,
            })?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let status = child.wait().map_err(|source| RepositoryError::Io {
            operation: "wait for exact source archive",
            path: self.root.display().to_string(),
            source,
        })?;
        if !status.success() {
            return Err(RepositoryError::Command {
                operation: "archive exact committed source",
                status,
            });
        }
        Ok(Sha256Digest::new(hasher.finalize().into()))
    }

    pub(super) fn head_commit(&self) -> Result<String, RepositoryError> {
        let output = self.git_output(&["rev-parse", "HEAD"])?;
        if !output.status.success() {
            return Err(RepositoryError::Command {
                operation: "resolve candidate HEAD",
                status: output.status,
            });
        }
        String::from_utf8(output.stdout)
            .map(|value| value.trim().to_owned())
            .map_err(|_| RepositoryError::Encoding("HEAD identity"))
    }

    fn git_output(&self, arguments: &[&str]) -> Result<std::process::Output, RepositoryError> {
        Command::new("git")
            .current_dir(&self.root)
            .args(arguments)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| RepositoryError::Io {
                operation: "run Git",
                path: self.root.display().to_string(),
                source,
            })
    }
}
