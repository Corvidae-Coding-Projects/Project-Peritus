//! Exact candidate-source identity and bounded file observations.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::Digest as _;

use crate::hex_digest;

use super::error::ControllerError;

pub(super) fn canonical_candidate_root(path: &Path) -> Result<PathBuf, ControllerError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| ControllerError::io("canonicalize candidate root", path, error))?;
    if !canonical.join("Cargo.toml").is_file() || !canonical.join("architecture.toml").is_file() {
        return Err(ControllerError::protocol("candidate root is not a Peritus workspace"));
    }
    Ok(canonical)
}

pub(super) fn verify_source_digest(root: &Path, expected: &str) -> Result<String, ControllerError> {
    verify_clean_checkout(root)?;
    let actual = source_tree_digest(root)?;
    if actual != expected {
        return Err(ControllerError::protocol(
            "git archive HEAD does not match the request candidate source digest",
        ));
    }
    Ok(actual)
}

fn verify_clean_checkout(root: &Path) -> Result<(), ControllerError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| ControllerError::io("inspect exact candidate checkout", root, error))?;
    if !output.status.success() {
        return Err(ControllerError::protocol("git status failed for the candidate checkout"));
    }
    if !output.stdout.is_empty() {
        return Err(ControllerError::protocol(
            "candidate checkout differs from its committed source identity",
        ));
    }
    Ok(())
}

pub(super) fn source_tree_digest(root: &Path) -> Result<String, ControllerError> {
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["archive", "--format=tar", "HEAD"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| ControllerError::io("start exact source archive", root, error))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ControllerError::protocol("git archive stdout is unavailable"))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = stdout
            .read(&mut buffer)
            .map_err(|error| ControllerError::io("read exact source archive", root, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let status = child
        .wait()
        .map_err(|error| ControllerError::io("wait for exact source archive", root, error))?;
    if !status.success() {
        return Err(ControllerError::protocol("git archive HEAD failed"));
    }
    Ok(hex_digest(peritus_types::Sha256Digest::new(hasher.finalize().into())))
}
