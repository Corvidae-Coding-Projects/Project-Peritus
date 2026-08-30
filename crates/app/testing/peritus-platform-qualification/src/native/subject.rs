//! `FreshSubjectFactory` backed by a reviewed native H2 controller.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tempfile::{Builder, TempDir};

use crate::{
    CleanupObservation, FreshSubjectFactory, QualificationError, QualificationSubject,
    QualificationTarget, ScenarioId, ScenarioObservation, ScenarioRequest, ScenarioSpec,
    Sha256Digest, digest_file,
};

use super::package::stage as stage_package;
use super::process::{ProcessRequest, execute, read_document};
use super::protocol::{
    CleanupValidation, NativeCleanupDocument, NativeRequestDocument, NativeResponseDocument,
    ResponseValidation,
};
use super::{NativeControllerLimits, cleanup_error, native_error};

/// Provisions one private package copy and one owned native controller for every H2 scenario.
pub struct NativePlatformFactory {
    controller: PathBuf,
    controller_digest: Sha256Digest,
    package_source: PathBuf,
    scratch_parent: PathBuf,
    artifact_parent: PathBuf,
    limits: NativeControllerLimits,
    next_subject: u64,
}

impl NativePlatformFactory {
    /// Validates the reviewed controller, staged release source, and campaign-owned directories.
    ///
    /// # Errors
    ///
    /// Rejects a non-file controller, non-directory package/scratch/artifact roots, or paths that
    /// cannot be canonicalized before qualification begins.
    pub fn new(
        controller: impl AsRef<Path>,
        package_source: impl AsRef<Path>,
        scratch_parent: impl AsRef<Path>,
        artifact_parent: impl AsRef<Path>,
        limits: NativeControllerLimits,
    ) -> Result<Self, QualificationError> {
        let controller = canonical_file(controller.as_ref(), "native H2 controller")?;
        let package_source = canonical_directory(package_source.as_ref(), "package source")?;
        let scratch_parent = canonical_directory(scratch_parent.as_ref(), "scratch parent")?;
        let artifact_parent =
            canonical_directory(artifact_parent.as_ref(), "retained-artifact parent")?;
        let controller_digest = digest_file(&controller, limits.package_artifact_bytes())?.sha256();
        Ok(Self {
            controller,
            controller_digest,
            package_source,
            scratch_parent,
            artifact_parent,
            limits,
            next_subject: 1,
        })
    }
}

impl FreshSubjectFactory for NativePlatformFactory {
    fn create(
        &mut self,
        target: QualificationTarget,
        scenario: ScenarioId,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError> {
        let sequence = self.next_subject;
        self.next_subject = self.next_subject.checked_add(1).ok_or_else(|| {
            native_error("provision native H2 subject", "fresh-subject sequence exhausted")
        })?;
        let temporary = Builder::new()
            .prefix("peritus-h2-")
            .tempdir_in(&self.scratch_parent)
            .map_err(|error| {
                native_error("provision native H2 subject", format!("create private root: {error}"))
            })?;
        let root = fs::canonicalize(temporary.path()).map_err(|error| {
            native_error(
                "provision native H2 subject",
                format!("canonicalize private root: {error}"),
            )
        })?;
        create_runtime_directories(&root)?;
        let staged_controller = root.join(staged_controller_name());
        fs::copy(&self.controller, &staged_controller).map_err(|error| {
            native_error(
                "provision native H2 subject",
                format!("stage reviewed controller: {error}"),
            )
        })?;
        let staged_digest =
            digest_file(&staged_controller, self.limits.package_artifact_bytes())?.sha256();
        if staged_digest != self.controller_digest {
            return Err(native_error(
                "provision native H2 subject",
                "reviewed controller changed while the subject was being provisioned",
            ));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                native_error("provision native H2 subject", format!("system clock: {error}"))
            })?
            .as_nanos();
        let subject_id = format!("h2-{}-{sequence}-{nonce}", std::process::id());
        let artifact_root = create_artifact_root(&self.artifact_parent, &subject_id)?;
        Ok(Box::new(NativePlatformSubject {
            subject_id,
            target,
            scenario,
            controller: staged_controller,
            controller_digest: staged_digest,
            package_source: self.package_source.clone(),
            root,
            artifact_root,
            limits: self.limits,
            temporary: Some(temporary),
            execution: None,
        }))
    }
}

struct NativePlatformSubject {
    subject_id: String,
    target: QualificationTarget,
    scenario: ScenarioId,
    controller: PathBuf,
    controller_digest: Sha256Digest,
    package_source: PathBuf,
    root: PathBuf,
    artifact_root: PathBuf,
    limits: NativeControllerLimits,
    temporary: Option<TempDir>,
    execution: Option<ExecutionContext>,
}

struct ExecutionContext {
    request_bytes: Vec<u8>,
    cleanup_path: PathBuf,
    scenario: ScenarioSpec,
    artifact_paths: BTreeSet<String>,
    artifact_bytes: u64,
}

impl QualificationSubject for NativePlatformSubject {
    fn subject_id(&self) -> &str {
        &self.subject_id
    }

    fn execute(
        &mut self,
        request: ScenarioRequest<'_>,
    ) -> Result<ScenarioObservation, QualificationError> {
        if self.execution.is_some()
            || request.target() != self.target
            || request.scenario().id() != self.scenario
        {
            return Err(native_error(
                "execute native H2 scenario",
                "request reused a subject or differed from its provisioned target and scenario",
            ));
        }
        let package_root = self.root.join("package");
        stage_package(&self.package_source, &package_root, request.manifest(), self.limits)?;
        let controller_sha256 = self.controller_digest.to_hex();
        let request_bytes = NativeRequestDocument::encode(
            &self.subject_id,
            &controller_sha256,
            request,
            self.limits,
        )?;
        let request_path = self.root.join("request.json");
        let response_path = self.root.join("response.json");
        let cleanup_path = self.root.join("cleanup-response.json");
        fs::write(&request_path, &request_bytes).map_err(|error| {
            native_error("write native H2 request", format!("request file: {error}"))
        })?;
        fs::write(self.root.join("package-manifest.toml"), request.manifest().canonical_bytes())
            .map_err(|error| {
                native_error("write native H2 request", format!("manifest file: {error}"))
            })?;
        let request_sha256 = crate::digest_bytes(&request_bytes).sha256().to_hex();
        self.execution = Some(ExecutionContext {
            request_bytes: request_bytes.clone(),
            cleanup_path: cleanup_path.clone(),
            scenario: request.scenario(),
            artifact_paths: BTreeSet::new(),
            artifact_bytes: 0,
        });
        let process = execute(
            ProcessRequest {
                executable: &self.controller,
                root: &self.root,
                package_root: &package_root,
                artifact_root: &self.artifact_root,
                request_path: &request_path,
                response_path: &response_path,
                cleanup_path: &cleanup_path,
                subject_id: &self.subject_id,
                request_sha256: &request_sha256,
            },
            self.limits,
        )?;
        let response_bytes =
            read_document(&response_path, self.limits.response_bytes(), "scenario response")?;
        let elapsed_millis = u64::try_from(process.elapsed.as_millis()).unwrap_or(u64::MAX);
        let exit_code = process.status.code().unwrap_or(-1);
        let response = NativeResponseDocument::parse_and_validate(
            &response_bytes,
            ResponseValidation {
                request_bytes: &request_bytes,
                subject_id: &self.subject_id,
                scenario: request.scenario(),
                artifact_root: &self.artifact_root,
                limits: self.limits,
                elapsed_millis,
                output_bytes: process.output_bytes,
                exit_code,
            },
        )?;
        if response.observation.outcome() == crate::ObservationOutcome::Passed
            && !process.status.success()
        {
            return Err(native_error(
                "validate native H2 response",
                "controller claimed pass with a nonzero process status",
            ));
        }
        let context = self.execution.as_mut().expect("execution context was installed");
        context.artifact_paths = response.artifact_paths;
        context.artifact_bytes = response.artifact_bytes;
        Ok(response.observation)
    }

    fn close(mut self: Box<Self>) -> Result<CleanupObservation, QualificationError> {
        let cleanup_result = self.validate_cleanup();
        let temporary = self.temporary.take().ok_or_else(|| {
            cleanup_error("clean native H2 subject", "private root was already consumed")
        })?;
        temporary.close().map_err(|error| {
            cleanup_error("clean native H2 subject", format!("remove private root: {error}"))
        })?;
        if self.root.exists() {
            return Err(cleanup_error(
                "clean native H2 subject",
                "private subject root remained after cleanup",
            ));
        }
        if !fs::metadata(&self.artifact_root).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(cleanup_error(
                "clean native H2 subject",
                "retained-artifact root was not preserved",
            ));
        }
        cleanup_result
    }
}

impl NativePlatformSubject {
    fn validate_cleanup(&self) -> Result<CleanupObservation, QualificationError> {
        let context = self.execution.as_ref().ok_or_else(|| {
            cleanup_error("clean native H2 subject", "scenario execution never started")
        })?;
        let cleanup_bytes =
            read_document(&context.cleanup_path, self.limits.response_bytes(), "cleanup response")?;
        NativeCleanupDocument::parse_and_validate(
            &cleanup_bytes,
            CleanupValidation {
                request_bytes: &context.request_bytes,
                subject_id: &self.subject_id,
                scenario: context.scenario,
                artifact_root: &self.artifact_root,
                prior_paths: &context.artifact_paths,
                prior_bytes: context.artifact_bytes,
                limits: self.limits,
            },
        )
    }
}

impl Drop for NativePlatformSubject {
    fn drop(&mut self) {
        if let Some(temporary) = self.temporary.take() {
            let _ = temporary.close();
        }
    }
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf, QualificationError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        native_error("configure native H2 subject", format!("canonicalize {label}: {error}"))
    })?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_file()) {
        return Err(native_error(
            "configure native H2 subject",
            format!("{label} is not a regular file"),
        ));
    }
    Ok(canonical)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, QualificationError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        native_error("configure native H2 subject", format!("canonicalize {label}: {error}"))
    })?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(native_error(
            "configure native H2 subject",
            format!("{label} is not an existing directory"),
        ));
    }
    Ok(canonical)
}

fn create_runtime_directories(root: &Path) -> Result<(), QualificationError> {
    for name in ["tmp", "config", "state", "data", "local-app-data", "app-data"] {
        fs::create_dir(root.join(name)).map_err(|error| {
            native_error("provision native H2 subject", format!("create {name}: {error}"))
        })?;
    }
    Ok(())
}

fn create_artifact_root(parent: &Path, subject_id: &str) -> Result<PathBuf, QualificationError> {
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
            "provision native H2 subject",
            format!("create retained-artifact root: {error}"),
        )
    })?;
    fs::canonicalize(&path).map_err(|error| {
        native_error(
            "provision native H2 subject",
            format!("canonicalize retained-artifact root: {error}"),
        )
    })
}

const fn staged_controller_name() -> &'static str {
    if cfg!(target_os = "windows") { "platform-controller.exe" } else { "platform-controller" }
}
