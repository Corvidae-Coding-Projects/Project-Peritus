//! `FreshSubjectFactory` backed by a reviewed native probe executable.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tempfile::{Builder, TempDir};

use crate::{
    CancellationToken, CleanupObservation, FreshSubjectFactory, NativeExecutionReceipt,
    ProbeObservation, ProbeRequest, ProbeSpec, QualificationError, QualificationLimits,
    QualificationSubject, ResourceUsage, digest_bytes, hex_digest,
};

use super::config::HostFingerprint;
use super::process::{ProcessRequest, execute, read_response, sha256_file};
use super::protocol::{
    MAX_RESPONSE_BYTES, NativeProbeRequestDocument, NativeProbeResponseDocument,
};
use super::{cleanup_error, native_error};

/// Provisions one private subject root and one owned native process for every H0 probe.
pub struct NativeProbeFactory {
    executor: PathBuf,
    executor_digest: peritus_types::Sha256Digest,
    candidate_root: PathBuf,
    scratch_parent: PathBuf,
    artifact_parent: PathBuf,
    host: HostFingerprint,
    next_subject: u64,
}

impl NativeProbeFactory {
    /// Validates the reviewed executor, private scratch parent, and retained-artifact parent.
    ///
    /// # Errors
    ///
    /// Rejects a non-file executor, a non-directory parent, or paths that cannot be canonicalized
    /// before the campaign begins.
    pub fn new(
        executor: impl AsRef<Path>,
        candidate_root: impl AsRef<Path>,
        scratch_parent: impl AsRef<Path>,
        artifact_parent: impl AsRef<Path>,
        host: HostFingerprint,
    ) -> Result<Self, QualificationError> {
        let executor = canonical_file(executor.as_ref(), "native H0 executor")?;
        let candidate_root = canonical_directory(candidate_root.as_ref(), "exact candidate root")?;
        let scratch_parent = canonical_directory(scratch_parent.as_ref(), "scratch parent")?;
        let artifact_parent =
            canonical_directory(artifact_parent.as_ref(), "retained-artifact parent")?;
        let executor_digest = sha256_file(&executor)?;
        Ok(Self {
            executor,
            executor_digest,
            candidate_root,
            scratch_parent,
            artifact_parent,
            host,
            next_subject: 1,
        })
    }
}

impl FreshSubjectFactory for NativeProbeFactory {
    fn create(
        &mut self,
        _candidate: crate::IntegratedCandidate,
        _spec: ProbeSpec,
        _limits: QualificationLimits,
        cancellation: &CancellationToken,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError> {
        if cancellation.is_cancelled() {
            return Err(native_error(
                "provision native H0 subject",
                "campaign cancellation requested",
            ));
        }
        let sequence = self.next_subject;
        self.next_subject = self.next_subject.checked_add(1).ok_or_else(|| {
            native_error("provision native H0 subject", "fresh-subject sequence exhausted")
        })?;
        let temporary = Builder::new()
            .prefix("peritus-h0-")
            .tempdir_in(&self.scratch_parent)
            .map_err(|error| {
                native_error("provision native H0 subject", format!("create private root: {error}"))
            })?;
        let root = fs::canonicalize(temporary.path()).map_err(|error| {
            native_error(
                "provision native H0 subject",
                format!("canonicalize private root: {error}"),
            )
        })?;
        let staged_executor = root.join(staged_executor_name());
        fs::copy(&self.executor, &staged_executor).map_err(|error| {
            native_error("provision native H0 subject", format!("stage reviewed executor: {error}"))
        })?;
        let staged_digest = sha256_file(&staged_executor)?;
        if staged_digest != self.executor_digest {
            return Err(native_error(
                "provision native H0 subject",
                "reviewed executor changed while the campaign was being provisioned",
            ));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                native_error("provision native H0 subject", format!("system clock: {error}"))
            })?
            .as_nanos();
        let subject_id = format!("h0-{}-{sequence}-{nonce}", std::process::id());
        let artifact_root = create_retained_artifact_root(&self.artifact_parent, &subject_id)?;
        Ok(Box::new(NativeProbeSubject {
            subject_id,
            executor: staged_executor,
            executor_digest: staged_digest,
            candidate_root: self.candidate_root.clone(),
            host: self.host,
            root,
            artifact_root,
            temporary: Some(temporary),
        }))
    }
}

struct NativeProbeSubject {
    subject_id: String,
    executor: PathBuf,
    executor_digest: peritus_types::Sha256Digest,
    candidate_root: PathBuf,
    host: HostFingerprint,
    root: PathBuf,
    artifact_root: PathBuf,
    temporary: Option<TempDir>,
}

impl QualificationSubject for NativeProbeSubject {
    fn subject_id(&self) -> &str {
        &self.subject_id
    }

    fn execute(
        &mut self,
        request: ProbeRequest<'_>,
    ) -> Result<ProbeObservation, QualificationError> {
        let request_bytes = NativeProbeRequestDocument::encode(request, &self.subject_id)?;
        let request_path = self.root.join("request.json");
        let response_path = self.root.join("response.json");
        fs::write(&request_path, &request_bytes).map_err(|error| {
            native_error("write native H0 request", format!("request file: {error}"))
        })?;
        let request_sha256 = hex_digest(digest_bytes(&request_bytes));
        let process = execute(
            ProcessRequest {
                executable: &self.executor,
                root: &self.root,
                request_path: &request_path,
                response_path: &response_path,
                artifact_root: &self.artifact_root,
                candidate_root: &self.candidate_root,
                subject_id: &self.subject_id,
                request_sha256: &request_sha256,
            },
            request.limits(),
            request.cancellation(),
        )?;
        let response_bytes = read_response(&response_path, MAX_RESPONSE_BYTES)?;
        let response = NativeProbeResponseDocument::parse_and_validate(
            &response_bytes,
            &request_bytes,
            request,
            &self.subject_id,
            &self.artifact_root,
        )?;
        if response.outcome == crate::ProbeOutcome::Passed && !process.status.success() {
            return Err(native_error(
                "validate native H0 response",
                "executor claimed pass with a nonzero process status",
            ));
        }
        let elapsed_millis = u64::try_from(process.elapsed.as_millis()).unwrap_or(u64::MAX);
        let usage = ResourceUsage::new(
            response.usage.elapsed_millis().max(elapsed_millis),
            response.usage.process_count().max(1),
            response.usage.peak_memory_bytes(),
            response.usage.output_bytes().max(process.output_bytes),
            response.usage.artifact_count(),
        );
        let exit_code = process.status.code().unwrap_or(-1);
        let command_digest = command_digest(
            &self.executor,
            self.executor_digest,
            &request_bytes,
            &self.subject_id,
            &self.artifact_root,
            &self.candidate_root,
        );
        let receipt = NativeExecutionReceipt::from_native_observation(
            self.executor_digest,
            self.host.digest(),
            command_digest,
            exit_code,
            response.native_sandbox_observed,
            usage,
            response.evidence,
        )?;
        Ok(ProbeObservation::from_native_execution(
            request.candidate(),
            request.spec().id(),
            response.outcome,
            receipt,
        ))
    }

    fn cleanup(mut self: Box<Self>) -> Result<CleanupObservation, QualificationError> {
        let temporary = self.temporary.take().ok_or_else(|| {
            cleanup_error("clean native H0 subject", "private root was already consumed")
        })?;
        temporary.close().map_err(|error| {
            cleanup_error("clean native H0 subject", format!("remove private root: {error}"))
        })?;
        if self.root.exists() {
            return Err(cleanup_error(
                "clean native H0 subject",
                "private subject root remained after cleanup",
            ));
        }
        if !fs::metadata(&self.artifact_root).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(cleanup_error(
                "clean native H0 subject",
                "retained-artifact root was not preserved",
            ));
        }
        let mut evidence = b"peritus/h0/native-cleanup/v1\0".to_vec();
        evidence.extend_from_slice(self.subject_id.as_bytes());
        evidence.extend_from_slice(self.executor_digest.as_bytes());
        evidence.extend_from_slice(self.artifact_root.to_string_lossy().as_bytes());
        CleanupObservation::new(self.subject_id.clone(), 0, 0, 0, 0, digest_bytes(&evidence))
    }
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf, QualificationError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        native_error("configure native H0 subject", format!("canonicalize {label}: {error}"))
    })?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_file()) {
        return Err(native_error(
            "configure native H0 subject",
            format!("{label} is not a regular file"),
        ));
    }
    Ok(canonical)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, QualificationError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        native_error("configure native H0 subject", format!("canonicalize {label}: {error}"))
    })?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(native_error(
            "configure native H0 subject",
            format!("{label} is not an existing directory"),
        ));
    }
    Ok(canonical)
}

fn create_retained_artifact_root(
    parent: &Path,
    subject_id: &str,
) -> Result<PathBuf, QualificationError> {
    let path = parent.join(subject_id);
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder.create(&path).map_err(|error| {
        native_error(
            "provision native H0 subject",
            format!("create retained-artifact root: {error}"),
        )
    })?;
    fs::canonicalize(&path).map_err(|error| {
        native_error(
            "provision native H0 subject",
            format!("canonicalize retained-artifact root: {error}"),
        )
    })
}

fn command_digest(
    executor: &Path,
    executor_digest: peritus_types::Sha256Digest,
    request: &[u8],
    subject_id: &str,
    artifact_root: &Path,
    candidate_root: &Path,
) -> peritus_types::Sha256Digest {
    let mut bytes = b"peritus/h0/native-command/v3\0".to_vec();
    bytes.extend_from_slice(executor_digest.as_bytes());
    bytes.extend_from_slice(executor.to_string_lossy().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(subject_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(artifact_root.to_string_lossy().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(candidate_root.to_string_lossy().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(request);
    digest_bytes(&bytes)
}

const fn staged_executor_name() -> &'static str {
    if cfg!(target_os = "windows") { "probe-executor.exe" } else { "probe-executor" }
}
