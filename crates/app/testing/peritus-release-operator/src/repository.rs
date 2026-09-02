//! Exact candidate and source-tree observations.

use std::{
    env,
    io::Read as _,
    path::Path,
    process::{Command, Stdio},
};

use peritus_release_artifacts::{
    CandidateCommit, PlatformTriple, ReleaseBinding, ReleaseVersion, Sha256Digest, ToolchainId,
};
use serde::Deserialize;
use sha2::Digest as _;

use crate::error::OperatorError;

#[derive(Deserialize)]
struct Manifest {
    workspace: Workspace,
}

#[derive(Deserialize)]
struct Workspace {
    package: WorkspacePackage,
    metadata: WorkspaceMetadata,
}

#[derive(Deserialize)]
struct WorkspacePackage {
    version: String,
}

#[derive(Deserialize)]
struct WorkspaceMetadata {
    peritus: PeritusMetadata,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PeritusMetadata {
    rust_toolchain: String,
    verus_version: String,
    vstd_revision: String,
}

pub fn root() -> Result<std::path::PathBuf, OperatorError> {
    let current = env::current_dir()
        .map_err(|error| OperatorError::io("read current directory", ".", error))?;
    if current.join("Cargo.toml").is_file() && current.join("architecture.toml").is_file() {
        Ok(current)
    } else {
        Err(OperatorError::metadata("run the release operator from the workspace root"))
    }
}

pub fn binding(root: &Path) -> Result<ReleaseBinding, OperatorError> {
    let metadata = manifest(root)?;
    let commit_text =
        environment("GITHUB_SHA").or_else(|_| git_text(root, &["rev-parse", "HEAD"]))?;
    let head = git_text(root, &["rev-parse", "HEAD"])?;
    if commit_text != head {
        return Err(OperatorError::metadata("GITHUB_SHA does not equal the checked-out candidate"));
    }
    let tag = environment("GITHUB_REF_NAME")?;
    if tag != format!("v{}", metadata.workspace.package.version) {
        return Err(OperatorError::metadata("release tag does not equal the workspace version"));
    }
    let pins = metadata.workspace.metadata.peritus;
    let toolchain = format!(
        "rust-{}-verus-{}-vstd-{}",
        pins.rust_toolchain, pins.verus_version, pins.vstd_revision
    );
    Ok(ReleaseBinding::new(
        CandidateCommit::new(commit_text)?,
        ReleaseVersion::new(metadata.workspace.package.version)?,
        ToolchainId::new(toolchain)?,
        platform(root)?,
        source_tree_digest(root)?,
    ))
}

fn manifest(root: &Path) -> Result<Manifest, OperatorError> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| OperatorError::io("read workspace manifest", &path, error))?;
    toml::from_str(&text).map_err(OperatorError::from)
}

fn platform(root: &Path) -> Result<PlatformTriple, OperatorError> {
    let details = git_or_command_text(root, "rustc", &["--version", "--verbose"])?;
    let host = details
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| OperatorError::metadata("rustc did not report its host triple"))?;
    let runner = environment("ImageOS").or_else(|_| environment("RUNNER_OS"))?;
    let runner_arch = environment("RUNNER_ARCH")?;
    PlatformTriple::new(format!("{host}@{runner}-{runner_arch}")).map_err(OperatorError::from)
}

fn source_tree_digest(root: &Path) -> Result<Sha256Digest, OperatorError> {
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["archive", "--format=tar", "HEAD"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| OperatorError::io("start exact source archive", root, error))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| OperatorError::metadata("git archive stdout was unavailable"))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = stdout
            .read(&mut buffer)
            .map_err(|error| OperatorError::io("read exact source archive", root, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let status = child
        .wait()
        .map_err(|error| OperatorError::io("wait for exact source archive", root, error))?;
    if !status.success() {
        return Err(OperatorError::Command { operation: "archive exact source tree", status });
    }
    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}

fn git_text(root: &Path, args: &[&str]) -> Result<String, OperatorError> {
    git_or_command_text(root, "git", args)
}

fn git_or_command_text(root: &Path, program: &str, args: &[&str]) -> Result<String, OperatorError> {
    let output = Command::new(program)
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| OperatorError::io("run release metadata command", program, error))?;
    if !output.status.success() {
        return Err(OperatorError::Command {
            operation: "collect release metadata",
            status: output.status,
        });
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|_| OperatorError::metadata("release metadata command returned non-UTF-8 output"))
}

pub fn environment(name: &'static str) -> Result<String, OperatorError> {
    env::var(name).map_err(|_| OperatorError::metadata(format!("{name} is required")))
}
