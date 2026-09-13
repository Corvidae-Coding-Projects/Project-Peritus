//! Input identity for a local proof run; external check-producer authority is separate.

mod data;

use crate::error::{ErrorCode, XtaskError};
use crate::{metadata, source};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const SHARED_INPUTS: [&str; 14] = [
    ".cargo/config.toml",
    ".gitattributes",
    "Cargo.lock",
    "Cargo.toml",
    "architecture.toml",
    "rust-toolchain.toml",
    "toolchains.toml",
    "verification/actor-provenance.json",
    "verification/actors.toml",
    "verification/exclusions.toml",
    "verification/obligations.toml",
    "verification/proof-impact.toml",
    "verification/trust.toml",
    ".github/workflows/formal-governance.yml",
];

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(super) struct Snapshot {
    git_commit: String,
    git_tree: String,
    protected_base: Option<String>,
    matches_committed_sources: bool,
    source_sha256: BTreeMap<String, String>,
}

pub(super) fn capture(root: &Path) -> Result<Snapshot, XtaskError> {
    let policy = metadata::architecture_policy(root)?;
    let cargo = metadata::cargo_metadata(root)?;
    let roots: Vec<_> = cargo
        .packages
        .iter()
        .flat_map(|package| package.targets.iter().map(|target| target.src_path.clone()))
        .collect();
    let discovered = source::discover_compilation_sources(root, &policy, &roots)?;
    if !discovered.diagnostics.is_empty() {
        return Err(XtaskError::violations(
            ErrorCode::Trust,
            "proof-input-snapshot",
            discovered.diagnostics,
        ));
    }
    let embedded_data = data::discover(root, &discovered.files, &cargo)?;
    let mut paths: BTreeSet<_> = discovered.files.into_iter().collect();
    paths.extend(embedded_data);
    paths.extend(cargo.packages.iter().map(|package| package.manifest_path.clone()));
    paths.extend(SHARED_INPUTS.into_iter().map(|relative| root.join(relative)));
    let source_sha256 = fingerprint_paths(root, &paths)?;
    let tracked = git_bytes(root, &["ls-files", "--cached", "-z"])?;
    let tracked: BTreeSet<_> = tracked.split(|byte| *byte == 0).collect();
    let all_tracked = source_sha256.keys().all(|path| tracked.contains(path.as_bytes()));
    let clean = git_bytes(root, &["status", "--porcelain=v1", "--untracked-files=no"])?;
    let protected_base = std::env::var("PERITUS_PROOF_IMPACT_BASE").ok();
    let snapshot = Snapshot {
        git_commit: git_object(root, "HEAD^{commit}")?,
        git_tree: git_object(root, "HEAD^{tree}")?,
        protected_base,
        matches_committed_sources: all_tracked && clean.is_empty(),
        source_sha256,
    };
    if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        let candidate = std::env::var("GITHUB_SHA")
            .map_err(|_| invalid("GitHub proof run has no candidate SHA"))?;
        snapshot.validate_ci_identity(&candidate)?;
        let base = snapshot.protected_base.as_deref().expect("validated protected base");
        git_bytes(root, &["merge-base", "--is-ancestor", base, &snapshot.git_commit])?;
    }
    Ok(snapshot)
}

impl Snapshot {
    fn validate_ci_identity(&self, candidate: &str) -> Result<(), XtaskError> {
        if !full_object_id(candidate)
            || self.git_commit != candidate
            || !self.matches_committed_sources
            || self
                .protected_base
                .as_deref()
                .is_none_or(|base| !full_object_id(base) || base == candidate)
        {
            return Err(invalid(
                "GitHub proof inputs must be clean committed sources at the exact event SHA with a distinct full protected base identity",
            ));
        }
        Ok(())
    }

    pub(super) fn require_unchanged(&self, after: &Self) -> Result<(), XtaskError> {
        if self != after {
            return Err(invalid(
                "source inputs or Git identities changed during proof execution; rerun from a stable candidate",
            ));
        }
        Ok(())
    }
}

fn fingerprint_paths(
    root: &Path,
    paths: &BTreeSet<PathBuf>,
) -> Result<BTreeMap<String, String>, XtaskError> {
    paths
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| invalid("proof input is outside the workspace"))?;
            let relative = input_name(root, relative)?;
            let bytes =
                fs::read(path).map_err(|error| XtaskError::io("read proof input", path, error))?;
            let sha256 =
                Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut output, byte| {
                    write!(output, "{byte:02x}").expect("writing to a String cannot fail");
                    output
                });
            Ok((relative, sha256))
        })
        .collect()
}

fn input_name(root: &Path, relative: &Path) -> Result<String, XtaskError> {
    let mut components = Vec::new();
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(invalid("proof input path is not a normal relative path"));
        };
        components.push(name.to_str().ok_or_else(|| invalid("proof input path is not UTF-8"))?);
        current.push(name);
        let metadata = current
            .symlink_metadata()
            .map_err(|error| XtaskError::io("inspect proof input", &current, error))?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("proof input path passes through a symbolic link"));
        }
    }
    if components.is_empty() {
        return Err(invalid("proof input path is empty"));
    }
    Ok(components.join("/"))
}

fn git_object(root: &Path, revision: &str) -> Result<String, XtaskError> {
    let bytes = git_bytes(root, &["rev-parse", "--verify", revision])?;
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| invalid("Git object identity is not UTF-8"))?
        .trim();
    if !full_object_id(value) {
        return Err(invalid("Git returned an invalid full object identity"));
    }
    Ok(value.to_owned())
}

fn full_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>, XtaskError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| XtaskError::io("read proof Git identity from", root, error))?;
    if !output.status.success() {
        return Err(invalid(format!("Git proof identity command failed: {}", output.status)));
    }
    Ok(output.stdout)
}

fn invalid(message: impl Into<String>) -> XtaskError {
    XtaskError::metadata(message)
}

#[cfg(test)]
mod tests;
