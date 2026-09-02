//! Reviewed candidate-bound controller for the 42 native H0 assertions.

mod args;
mod error;
mod execute;
mod inventory;
mod plan;
mod request;
mod source;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::{digest_bytes, hex_digest};

use self::args::Options;
use self::error::ControllerError;

const ARTIFACT_NAME: &str = "probe-evidence.json";
const MAX_REQUEST_BYTES: u64 = 256 * 1024;

/// Terminal status of one authenticated native probe controller invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControllerStatus {
    /// Every probe-specific candidate assertion passed within the request limits.
    Passed,
    /// The controller completed and retained evidence for at least one failed assertion.
    Failed,
}

/// Parses native protocol arguments and executes exactly one closed-catalog candidate probe.
///
/// # Errors
///
/// Rejects malformed arguments, path substitution, request or source digest mismatch, unsupported
/// native targets, invalid inventories, effect-boundary failures, and publication collisions.
pub fn run_from_env() -> Result<ControllerStatus, Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    run(&arguments).map_err(Into::into)
}

fn run(arguments: &[OsString]) -> Result<ControllerStatus, ControllerError> {
    let started = Instant::now();
    let options = Options::parse(arguments)?;
    let paths = BoundPaths::new(&options)?;
    let request_bytes = read_bounded(&paths.request, MAX_REQUEST_BYTES)?;
    let request = request::decode(&request_bytes, &options)?;
    let source_sha256 =
        source::verify_source_digest(&paths.candidate_root, &request.source_sha256)?;
    let plan = plan::for_probe(request.spec.id());
    let execution = execute::run(&plan, &paths.candidate_root, request.limits.max_output_bytes())?;
    let elapsed_millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let resources_within = elapsed_millis <= request.limits.max_duration_millis()
        && execution.process_count <= request.limits.max_processes()
        && execution.peak_memory_bytes <= request.limits.max_peak_memory_bytes()
        && execution.output_bytes <= request.limits.max_output_bytes();
    let passed = execution.passed && resources_within;
    let artifact = ArtifactDocument {
        schema: "peritus.h0-native-probe-evidence.v1",
        subject_id: &request.subject_id,
        probe_id: request.spec.id().as_str(),
        candidate_source_sha256: &source_sha256,
        request_sha256: &request.request_sha256,
        assertions_passed: execution.passed,
        resources_within_limits: resources_within,
        records: &execution.records,
    };
    let mut artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    artifact_bytes.push(b'\n');
    let artifact_path = paths.artifact_root.join(ARTIFACT_NAME);
    write_new(&artifact_path, &artifact_bytes)?;
    let artifact_sha256 = hex_digest(digest_bytes(&artifact_bytes));
    let native_sandbox_observed = plan.native_sandbox && execution.passed;
    let response = ResponseDocument {
        schema_version: 1,
        subject_id: &request.subject_id,
        request_sha256: &request.request_sha256,
        probe_id: request.spec.id().as_str(),
        outcome: if passed { "passed" } else { "failed" },
        native_sandbox_observed,
        usage: UsageDocument {
            elapsed_millis,
            process_count: execution.process_count,
            peak_memory_bytes: execution.peak_memory_bytes,
            output_bytes: execution.output_bytes,
            artifact_count: 1,
        },
        evidence: vec![
            EvidenceDocument::Fact { label: "assertion.source-bound", value: true },
            EvidenceDocument::Fact { label: "assertion.checks-passed", value: execution.passed },
            EvidenceDocument::Fact { label: "assertion.resources-within", value: resources_within },
            EvidenceDocument::Count {
                label: "assertion.check-count",
                value: u64::try_from(execution.records.len()).unwrap_or(u64::MAX),
            },
            EvidenceDocument::Code {
                label: "assertion.result",
                value: if passed { "passed" } else { "failed" },
            },
            EvidenceDocument::Digest {
                label: "assertion.raw",
                path: ARTIFACT_NAME,
                sha256: &artifact_sha256,
                bytes: u64::try_from(artifact_bytes.len()).unwrap_or(u64::MAX),
            },
        ],
    };
    let mut response_bytes = serde_json::to_vec_pretty(&response)?;
    response_bytes.push(b'\n');
    publish_response(&paths.response, &response_bytes)?;
    Ok(if passed { ControllerStatus::Passed } else { ControllerStatus::Failed })
}

struct BoundPaths {
    request: PathBuf,
    response: PathBuf,
    artifact_root: PathBuf,
    candidate_root: PathBuf,
}

impl BoundPaths {
    fn new(options: &Options) -> Result<Self, ControllerError> {
        let subject_root = canonical_directory(&options.subject_root, "subject root")?;
        let artifact_root = canonical_directory(&options.artifact_root, "artifact root")?;
        let candidate_root = source::canonical_candidate_root(&options.candidate_root)?;
        let request = fs::canonicalize(&options.request).map_err(|error| {
            ControllerError::io("canonicalize request", &options.request, error)
        })?;
        if !request.starts_with(&subject_root) || !request.is_file() {
            return Err(ControllerError::protocol("request is outside the subject root"));
        }
        if options.response.exists() {
            return Err(ControllerError::protocol("response path already exists"));
        }
        let response_parent = options
            .response
            .parent()
            .ok_or_else(|| ControllerError::protocol("response has no parent"))?;
        let canonical_parent = canonical_directory(response_parent, "response parent")?;
        if !canonical_parent.starts_with(&subject_root) {
            return Err(ControllerError::protocol("response is outside the subject root"));
        }
        Ok(Self { request, response: options.response.clone(), artifact_root, candidate_root })
    }
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, ControllerError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| ControllerError::io("canonicalize directory", path, error))?;
    if !canonical.is_dir() {
        return Err(ControllerError::protocol(format!("{label} is not a directory")));
    }
    Ok(canonical)
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, ControllerError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ControllerError::io("inspect request", path, error))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err(ControllerError::protocol("request is not a bounded regular file"));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)
        .map_err(|error| ControllerError::io("open request", path, error))?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ControllerError::io("read request", path, error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(ControllerError::protocol("request grew beyond its byte bound"));
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ControllerError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| ControllerError::io("create retained evidence", path, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| ControllerError::io("persist retained evidence", path, error))
}

fn publish_response(path: &Path, bytes: &[u8]) -> Result<(), ControllerError> {
    let parent =
        path.parent().ok_or_else(|| ControllerError::protocol("response has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| ControllerError::io("create response temporary", parent, error))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| ControllerError::io("write response temporary", path, error))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| ControllerError::io("publish response", path, error.error))?;
    Ok(())
}

#[derive(Serialize)]
struct ArtifactDocument<'a> {
    schema: &'static str,
    subject_id: &'a str,
    probe_id: &'static str,
    candidate_source_sha256: &'a str,
    request_sha256: &'a str,
    assertions_passed: bool,
    resources_within_limits: bool,
    records: &'a [execute::CheckRecord],
}

#[derive(Serialize)]
struct ResponseDocument<'a> {
    schema_version: u8,
    subject_id: &'a str,
    request_sha256: &'a str,
    probe_id: &'static str,
    outcome: &'static str,
    native_sandbox_observed: bool,
    usage: UsageDocument,
    evidence: Vec<EvidenceDocument<'a>>,
}

#[derive(Serialize)]
struct UsageDocument {
    elapsed_millis: u64,
    process_count: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifact_count: u32,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum EvidenceDocument<'a> {
    Fact { label: &'static str, value: bool },
    Count { label: &'static str, value: u64 },
    Digest { label: &'static str, path: &'static str, sha256: &'a str, bytes: u64 },
    Code { label: &'static str, value: &'static str },
}
