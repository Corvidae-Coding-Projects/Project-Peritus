//! Reviewed, bounded facts for the native host executing one H0 shard.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::repository::CandidateRepository;
use crate::{QualificationPlatform, hex_digest};

use super::PreparationError;
use super::candidate::PreparedCandidate;

#[derive(Serialize)]
struct HostFactsDocument<'a> {
    schema_version: u8,
    platform: &'static str,
    host_os: &'static str,
    host_arch: &'static str,
    candidate_commit: &'a str,
    candidate_source_sha256: String,
    rustc_verbose: String,
    controller_file: String,
    controller_sha256: String,
    controller_bytes: u64,
    runner_os: Option<String>,
    runner_arch: Option<String>,
    runner_image: Option<String>,
    runner_image_version: Option<String>,
}

pub(super) fn document(
    repository: &CandidateRepository,
    candidate: &PreparedCandidate,
    controller: &Path,
    platform: QualificationPlatform,
) -> Result<Vec<u8>, PreparationError> {
    let metadata = fs::symlink_metadata(controller).map_err(|source| {
        PreparationError::io("inspect reviewed controller", controller, source)
    })?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(PreparationError::Metadata(
            "reviewed controller must be a nonempty regular file",
        ));
    }
    let controller_file = controller
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(PreparationError::Metadata("controller file name is not UTF-8"))?
        .to_owned();
    let facts = HostFactsDocument {
        schema_version: 1,
        platform: platform.as_str(),
        host_os: env::consts::OS,
        host_arch: env::consts::ARCH,
        candidate_commit: &candidate.commit,
        candidate_source_sha256: hex_digest(candidate.candidate.source_digest()),
        rustc_verbose: rustc_verbose(repository.root())?,
        controller_file,
        controller_sha256: hex_digest(file_digest(controller)?),
        controller_bytes: metadata.len(),
        runner_os: environment("RUNNER_OS"),
        runner_arch: environment("RUNNER_ARCH"),
        runner_image: environment("ImageOS"),
        runner_image_version: environment("ImageVersion"),
    };
    let mut bytes = serde_json::to_vec_pretty(&facts)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn rustc_verbose(root: &Path) -> Result<String, PreparationError> {
    let output = Command::new("rustc")
        .current_dir(root)
        .args(["--version", "--verbose"])
        .stdin(Stdio::null())
        .output()
        .map_err(|source| PreparationError::io("inspect native Rust compiler", root, source))?;
    if !output.status.success() {
        return Err(PreparationError::Metadata("rustc --version --verbose failed"));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|_| PreparationError::Metadata("rustc version facts are not UTF-8"))
}

fn file_digest(path: &Path) -> Result<peritus_types::Sha256Digest, PreparationError> {
    use sha2::{Digest as _, Sha256};
    use std::io::Read as _;

    let mut file = fs::File::open(path)
        .map_err(|source| PreparationError::io("open reviewed controller", path, source))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| PreparationError::io("read reviewed controller", path, source))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(peritus_types::Sha256Digest::new(hasher.finalize().into()))
}

fn environment(name: &'static str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}
