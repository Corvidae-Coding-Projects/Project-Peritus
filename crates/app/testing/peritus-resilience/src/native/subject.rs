//! Fresh H1 subjects backed by one persistent reviewed controller each.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tempfile::{Builder, TempDir};

use crate::{
    CancellationToken, CleanupObservation, DisruptionObservation, PreparationObservation,
    QualificationConfig, QualificationFuture, RecoveryObservation, ResilienceSubject,
    ResilienceSubjectFactory, ScenarioSpec, SubjectDescriptor, SubjectError, SubjectErrorCode,
};

use super::controller::{ControllerHandle, LaunchRequest};
use super::protocol::{Stage, ValidatedStage, encode_request, parse_response};
use super::{NativeAdapterError, NativeControllerLimits, digest, subject_error};

/// Factory for release-candidate H1 subjects controlled by a reviewed native executable.
pub struct NativeResilienceFactory {
    executor: PathBuf,
    executor_digest: crate::EvidenceDigest,
    scratch_parent: PathBuf,
    artifact_parent: PathBuf,
    descriptor: SubjectDescriptor,
    config: QualificationConfig,
    limits: NativeControllerLimits,
    next_instance: AtomicU64,
}

impl NativeResilienceFactory {
    /// Validates the controller executable and both campaign-owned parent directories.
    ///
    /// # Errors
    ///
    /// Returns a typed configuration error when a path cannot be canonicalized, the controller is
    /// not a regular file, or either parent is not an existing directory.
    pub fn new(
        executor: impl AsRef<Path>,
        scratch_parent: impl AsRef<Path>,
        artifact_parent: impl AsRef<Path>,
        descriptor: SubjectDescriptor,
        config: QualificationConfig,
        limits: NativeControllerLimits,
    ) -> Result<Self, NativeAdapterError> {
        let executor = canonical_file(executor.as_ref(), "controller executable")?;
        let scratch_parent = canonical_directory(scratch_parent.as_ref(), "scratch parent")?;
        let artifact_parent =
            canonical_directory(artifact_parent.as_ref(), "retained-artifact parent")?;
        let executor_digest = digest::file(&executor)?;
        Ok(Self {
            executor,
            executor_digest,
            scratch_parent,
            artifact_parent,
            descriptor,
            config,
            limits,
            next_instance: AtomicU64::new(1),
        })
    }

    /// Returns the exact deterministic bounds serialized into every controller request.
    #[must_use]
    pub const fn config(&self) -> QualificationConfig {
        self.config
    }
}

impl ResilienceSubjectFactory<NativeResilienceSubject> for NativeResilienceFactory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _scenario: &'a ScenarioSpec,
        cancellation: CancellationToken,
    ) -> QualificationFuture<'a, Result<NativeResilienceSubject, SubjectError>> {
        Box::pin(std::future::ready(self.create_subject(cancellation)))
    }

    fn cleanup<'a>(
        &'a self,
        scenario: &'a ScenarioSpec,
        mut subject: NativeResilienceSubject,
    ) -> QualificationFuture<'a, Result<CleanupObservation, SubjectError>> {
        Box::pin(async move { subject.cleanup(scenario).await })
    }
}

impl NativeResilienceFactory {
    fn create_subject(
        &self,
        cancellation: CancellationToken,
    ) -> Result<NativeResilienceSubject, SubjectError> {
        if cancellation.is_cancelled() {
            return Err(setup_error("qualification cancellation was already requested"));
        }
        let sequence = self.next_instance.fetch_add(1, Ordering::AcqRel);
        if sequence == u64::MAX {
            return Err(setup_error("fresh-subject sequence exhausted"));
        }
        let temporary = Builder::new()
            .prefix("peritus-h1-")
            .tempdir_in(&self.scratch_parent)
            .map_err(|error| setup_error(format!("create private subject root: {error}")))?;
        let root = fs::canonicalize(temporary.path())
            .map_err(|error| setup_error(format!("canonicalize private subject root: {error}")))?;
        let staged_executor = root.join(staged_executor_name());
        fs::copy(&self.executor, &staged_executor)
            .map_err(|error| setup_error(format!("stage reviewed controller: {error}")))?;
        let staged_digest = digest::file(&staged_executor)
            .map_err(|error| setup_error(format!("digest staged controller: {error}")))?;
        if staged_digest != self.executor_digest {
            return Err(setup_error(
                "reviewed controller changed while the subject was being provisioned",
            ));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| setup_error(format!("read system clock: {error}")))?
            .as_nanos();
        let instance_id = format!("h1-{}-{sequence}-{nonce}", std::process::id());
        let artifact_root = create_artifact_root(&self.artifact_parent, &instance_id)?;
        let executor_sha256 = digest::hex(staged_digest);
        let build_sha256 = digest::hex(self.descriptor.build_digest());
        let controller = ControllerHandle::launch(
            LaunchRequest {
                executable: &staged_executor,
                subject_root: &root,
                artifact_root: &artifact_root,
                instance_id: &instance_id,
                subject_id: self.descriptor.id().as_str(),
                build_sha256: &build_sha256,
                executor_sha256: &executor_sha256,
            },
            self.limits,
            cancellation,
        )?;
        Ok(NativeResilienceSubject {
            instance_id,
            descriptor: self.descriptor.clone(),
            config: self.config,
            executor_sha256,
            root,
            artifact_root,
            temporary: Some(temporary),
            controller: Some(controller),
            next_sequence: 1,
        })
    }
}

/// One private H1 subject and its persistent owned controller process.
pub struct NativeResilienceSubject {
    instance_id: String,
    descriptor: SubjectDescriptor,
    config: QualificationConfig,
    executor_sha256: String,
    root: PathBuf,
    artifact_root: PathBuf,
    temporary: Option<TempDir>,
    controller: Option<ControllerHandle>,
    next_sequence: u8,
}

impl ResilienceSubject for NativeResilienceSubject {
    fn prepare<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<PreparationObservation, SubjectError>> {
        Box::pin(async move {
            match self.issue(Stage::Prepare, scenario).await? {
                ValidatedStage::Preparation(observation) => Ok(observation),
                _ => Err(protocol_stage_error()),
            }
        })
    }

    fn inject<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<DisruptionObservation, SubjectError>> {
        Box::pin(async move {
            match self.issue(Stage::Inject, scenario).await? {
                ValidatedStage::Injection(observation) => Ok(observation),
                _ => Err(protocol_stage_error()),
            }
        })
    }

    fn recover<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<RecoveryObservation, SubjectError>> {
        Box::pin(async move {
            match self.issue(Stage::Recover, scenario).await? {
                ValidatedStage::Recovery(observation) => Ok(observation),
                _ => Err(protocol_stage_error()),
            }
        })
    }
}

impl NativeResilienceSubject {
    async fn issue(
        &mut self,
        stage: Stage,
        scenario: &ScenarioSpec,
    ) -> Result<ValidatedStage, SubjectError> {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            subject_error(SubjectErrorCode::Observation, "stage sequence exhausted", false)
        })?;
        let request = encode_request(
            stage,
            sequence,
            &self.instance_id,
            &self.descriptor,
            &self.executor_sha256,
            scenario,
            self.config,
        )?;
        let request_sha256 = request.sha256.clone();
        let controller = self.controller.as_ref().ok_or_else(|| {
            subject_error(SubjectErrorCode::Supervision, "controller was already consumed", false)
        })?;
        let response = controller.command(stage, request).await?;
        parse_response(
            &response,
            stage,
            sequence,
            &request_sha256,
            &self.instance_id,
            scenario,
            &self.artifact_root,
        )
    }

    async fn cleanup(
        &mut self,
        scenario: &ScenarioSpec,
    ) -> Result<CleanupObservation, SubjectError> {
        let ValidatedStage::Cleanup(observation) = self.issue(Stage::Cleanup, scenario).await?
        else {
            return Err(protocol_stage_error());
        };
        let mut controller = self.controller.take().ok_or_else(|| {
            subject_error(SubjectErrorCode::Cleanup, "controller was already consumed", false)
        })?;
        controller.finish()?;
        let temporary = self.temporary.take().ok_or_else(|| {
            subject_error(SubjectErrorCode::Cleanup, "subject root was already consumed", false)
        })?;
        temporary.close().map_err(|error| {
            subject_error(
                SubjectErrorCode::Cleanup,
                format!("remove private subject root: {error}"),
                false,
            )
        })?;
        if self.root.exists() {
            return Err(subject_error(
                SubjectErrorCode::Cleanup,
                "private subject root remained after cleanup",
                false,
            ));
        }
        if !fs::metadata(&self.artifact_root).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(subject_error(
                SubjectErrorCode::Cleanup,
                "retained-artifact root was not preserved",
                false,
            ));
        }
        Ok(observation)
    }
}

impl Drop for NativeResilienceSubject {
    fn drop(&mut self) {
        drop(self.controller.take());
        if let Some(temporary) = self.temporary.take() {
            let _ = temporary.close();
        }
    }
}

fn canonical_file(path: &Path, label: &'static str) -> Result<PathBuf, NativeAdapterError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| NativeAdapterError::filesystem("canonicalize path", path, error))?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_file()) {
        return Err(NativeAdapterError::PathType {
            label,
            expected: "a regular file",
            path: canonical,
        });
    }
    Ok(canonical)
}

fn canonical_directory(path: &Path, label: &'static str) -> Result<PathBuf, NativeAdapterError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| NativeAdapterError::filesystem("canonicalize path", path, error))?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(NativeAdapterError::PathType {
            label,
            expected: "an existing directory",
            path: canonical,
        });
    }
    Ok(canonical)
}

fn create_artifact_root(parent: &Path, instance_id: &str) -> Result<PathBuf, SubjectError> {
    let path = parent.join(instance_id);
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder
        .create(&path)
        .map_err(|error| setup_error(format!("create retained-artifact root: {error}")))?;
    fs::canonicalize(&path)
        .map_err(|error| setup_error(format!("canonicalize retained-artifact root: {error}")))
}

fn setup_error(detail: impl Into<String>) -> SubjectError {
    subject_error(SubjectErrorCode::Setup, detail, false)
}

fn protocol_stage_error() -> SubjectError {
    subject_error(
        SubjectErrorCode::Observation,
        "controller returned the wrong stage payload",
        false,
    )
}

const fn staged_executor_name() -> &'static str {
    if cfg!(target_os = "windows") { "resilience-controller.exe" } else { "resilience-controller" }
}
