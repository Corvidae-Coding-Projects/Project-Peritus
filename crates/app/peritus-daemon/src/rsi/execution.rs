//! Credential-free local RSI executions through the committed C2 and native C3 gateway.
//!
//! Preparation does not confer execution authority. The dispatcher must bind its real admitted
//! evaluation claim to `plan()` and supply the resulting committed authorization at launch.
//! There is deliberately no unrestricted or reference-backend fallback.

mod materialize;
#[cfg(target_os = "linux")]
mod recovery;
#[cfg(target_os = "linux")]
mod sandbox;

#[cfg(target_os = "linux")]
pub(crate) use recovery::cleanup_empty_preparation;

use std::path::{Path, PathBuf};

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_process::{
    DeadlinePolicy, EnvironmentPlan, ExecutionAuthorizationRequest, ExecutionGateway,
    ExecutionIdentity, ExecutionPlan, GracefulAction, IoMode, OutputOverflowAction, OutputPolicy,
    OutputStream, OwnedProcess, ProcessControl, ProcessResourcePolicy, StdinPolicy, TerminalResult,
    WorkingDirectory, WorkspaceAccess,
};
use peritus_types::EventId;
use tempfile::TempDir;

/// Exact installed native isolation components. These paths are never candidate inputs.
#[derive(Clone, Debug)]
pub(crate) struct LocalProcessSandbox {
    pub(crate) bubblewrap: PathBuf,
    pub(crate) helper: PathBuf,
    pub(crate) cgroup_root: PathBuf,
}

/// A frozen executable and immutable supplementary files from the protected artifact store.
#[derive(Clone, Debug)]
pub(crate) struct ExecutableInput {
    pub(crate) digest: ArtifactDigest,
    pub(crate) size: u64,
    pub(crate) arguments: Vec<ExecutionArgument>,
    pub(crate) support_files: Vec<FrozenSupportFile>,
}

/// One frozen executable resource. A flat name prevents path and mount injection.
#[derive(Clone, Debug)]
pub(crate) struct FrozenSupportFile {
    pub(crate) name: String,
    pub(crate) digest: ArtifactDigest,
}

/// Only descriptor-owned tokens may select a role input path; task arguments remain literal.
#[derive(Clone, Debug)]
pub(crate) enum ExecutionArgument {
    Descriptor(String),
    Literal(String),
}

/// Files visible to a role. Candidate processes cannot receive the evaluator's sealed inputs.
#[derive(Clone, Debug)]
pub(crate) enum ProcessInputs {
    Candidate {
        public_task: ArtifactDigest,
    },
    Evaluator {
        sealed_task: ArtifactDigest,
        candidate_stdout: ArtifactDigest,
        candidate_stderr: ArtifactDigest,
        candidate_terminal: ArtifactDigest,
    },
}

/// Finite per-process ceilings; the evaluation scheduler owns campaign concurrency separately.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalProcessLimits {
    pub(crate) wall_millis: u64,
    pub(crate) cpu_millis: u64,
    pub(crate) memory_bytes: u64,
    pub(crate) disk_bytes: u64,
    pub(crate) output_bytes: u64,
    pub(crate) process_count: u64,
}

impl LocalProcessLimits {
    pub(super) fn resources(self) -> Result<ProcessResourcePolicy, String> {
        if self.wall_millis > 3_600_000
            || self.cpu_millis > 14_400_000
            || self.memory_bytes > 4 * 1024 * 1024 * 1024
            || self.disk_bytes > 512 * 1024 * 1024
            || self.output_bytes > 16 * 1024 * 1024
            || self.process_count > 64
        {
            return Err("RSI local-process ceilings exceed the supported bounded profile".into());
        }
        ProcessResourcePolicy::new(
            Some(self.wall_millis),
            Some(self.cpu_millis),
            self.memory_bytes,
            self.disk_bytes,
            Some(self.output_bytes),
            self.process_count,
            256,
            1,
        )
        .map_err(detail)
    }
}

/// An inert exact execution ready for committed authority binding.
pub(crate) struct PreparedExecution {
    plan: ExecutionPlan,
    directory: TempDir,
    files: Vec<materialize::MaterializedFile>,
    #[cfg(target_os = "linux")]
    checked: peritus_sandbox::CheckedSandboxPlan,
    #[cfg(target_os = "linux")]
    admission: peritus_sandbox::BackendAdmission,
    #[cfg(target_os = "linux")]
    backend: peritus_sandbox_linux::LinuxBackend,
}

impl PreparedExecution {
    pub(crate) const fn plan(&self) -> &ExecutionPlan {
        &self.plan
    }

    /// Exact owned scratch path for the durable rollout receipt before any process effect.
    pub(crate) fn scratch_directory(&self) -> &Path {
        self.directory.path()
    }

    /// Starts only after the existing C2 gateway consumes exact committed authority.
    pub(crate) fn launch(
        self,
        gateway: &ExecutionGateway,
        authorization: &ExecutionAuthorizationRequest<'_>,
    ) -> Result<OwnedExecution, String> {
        materialize::revalidate(&self.files)?;
        #[cfg(target_os = "linux")]
        {
            let output_bytes = self
                .plan
                .resource_policy()
                .output_bytes()
                .ok_or("RSI local execution unexpectedly lacks its finite output ceiling")?;
            let process = gateway
                .launch_with_backend(
                    authorization,
                    self.plan,
                    &self.checked,
                    &self.admission,
                    self.backend,
                )
                .map_err(detail)?;
            Ok(OwnedExecution { process, directory: self.directory, output_bytes })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (gateway, authorization);
            Err("RSI local-process execution requires the Linux private-filesystem native backend"
                .into())
        }
    }
}

/// Owns the complete process tree before removing its private input and working directories.
pub(crate) struct OwnedExecution {
    // The process is intentionally dropped first: cancellation/join precedes scratch removal.
    process: OwnedProcess,
    directory: TempDir,
    output_bytes: u64,
}

impl OwnedExecution {
    pub(crate) fn control(&self) -> ProcessControl {
        self.process.control()
    }

    /// Joins and durably publishes actual process output, retaining failed terminal outcomes.
    pub(crate) fn wait_and_publish(
        self,
        artifacts: &ArtifactStore,
        creating_event: EventId,
    ) -> Result<ProcessCompletion, String> {
        let control = self.process.control();
        let terminal = self.process.wait_and_publish(artifacts, creating_event).map_err(detail)?;
        let stdout = bounded_output(&control, OutputStream::Stdout, self.output_bytes)?;
        let stderr = bounded_output(&control, OutputStream::Stderr, self.output_bytes)?;
        if !terminal.tree_cleanup_complete() || !terminal.support_tasks_joined() {
            return Err(
                "RSI local-process teardown is incomplete; evaluation must not settle".into()
            );
        }
        // Keep the scratch owner alive until after publication and all stream reads.
        drop(self.directory);
        Ok(ProcessCompletion { terminal, stdout, stderr })
    }
}

/// Observed terminal and bytes. This is neither an evaluator verdict nor promotion authority.
pub(crate) struct ProcessCompletion {
    pub(crate) terminal: TerminalResult,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

/// Creates a private role-specific materialization and an inert restricted execution plan.
#[cfg(target_os = "linux")]
pub(crate) fn prepare(
    artifacts: &ArtifactStore,
    scratch_root: &Path,
    native: &LocalProcessSandbox,
    identity: ExecutionIdentity,
    executable: &ExecutableInput,
    inputs: &ProcessInputs,
    limits: LocalProcessLimits,
) -> Result<PreparedExecution, String> {
    use peritus_process::{CommandSpec, NativeSandboxBackend};
    use peritus_sandbox::{AdmissionProfile, admit_backend};
    let resources = limits.resources()?;
    let materialized = materialize::prepare(artifacts, scratch_root, executable, inputs)?;
    let config = peritus_sandbox_linux::LinuxBackendConfig::new(
        materialized.work.clone(),
        Vec::new(),
        native.bubblewrap.clone(),
        native.helper.clone(),
        native.cgroup_root.clone(),
        None,
    )
    .map_err(detail)?;
    let backend = peritus_sandbox_linux::LinuxBackend::new(config.with_private_filesystem())
        .map_err(detail)?;
    let checked = sandbox::compile(identity, &materialized, resources)?;
    let admission = admit_backend(&checked, backend.descriptor(), AdmissionProfile::Production)
        .map_err(detail)?;
    let working = WorkingDirectory::open(
        &materialized.work,
        identity.workspace_id(),
        identity.resource_id(),
        identity.environment_id(),
        identity.revision().workspace_generation(),
        identity.revision().workspace_revision(),
        WorkspaceAccess::Writable,
    )
    .map_err(detail)?;
    let output = OutputPolicy::new(
        4096.min(limits.output_bytes),
        (64 * 1024).min(limits.output_bytes),
        limits.output_bytes,
        8192,
        limits.output_bytes,
        limits.output_bytes,
        limits.output_bytes,
        OutputOverflowAction::Terminate,
    )
    .map_err(detail)?;
    let command = CommandSpec::new(
        materialized.executable.to_str().ok_or("RSI executable path is not UTF-8")?,
        materialized.arguments,
    )
    .map_err(detail)?;
    let plan = ExecutionPlan::new(
        identity,
        command,
        working,
        EnvironmentPlan::cleared(Vec::new()).map_err(detail)?,
        IoMode::Pipes,
        StdinPolicy::Closed,
        output,
        DeadlinePolicy::new(Some(limits.wall_millis), GracefulAction::Terminate, 100, 2000)
            .map_err(detail)?,
        resources,
        &checked,
        &admission,
    )
    .map_err(detail)?;
    Ok(PreparedExecution {
        plan,
        directory: materialized.directory,
        files: materialized.files,
        checked,
        admission,
        backend,
    })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn prepare(
    _artifacts: &ArtifactStore,
    _scratch_root: &Path,
    _native: &LocalProcessSandbox,
    _identity: ExecutionIdentity,
    _executable: &ExecutableInput,
    _inputs: &ProcessInputs,
    _limits: LocalProcessLimits,
) -> Result<PreparedExecution, String> {
    Err("RSI local-process execution requires the Linux private-filesystem native backend".into())
}

fn bounded_output(
    control: &ProcessControl,
    stream: OutputStream,
    ceiling: u64,
) -> Result<Vec<u8>, String> {
    let bound = usize::try_from(ceiling).map_err(|_| "RSI output ceiling exceeds address space")?;
    let (total, bytes) = control.spooled_stream_range(stream, 0, bound).map_err(detail)?;
    if total > ceiling || total != bytes.len() as u64 {
        return Err("RSI retained stream exceeds its exact bounded observation".into());
    }
    Ok(bytes)
}

fn detail(error: impl std::fmt::Display) -> String {
    format!("RSI local-process execution: {error}")
}
