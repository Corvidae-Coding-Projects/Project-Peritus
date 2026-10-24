//! Durable, host-owned Linux jobs for native modules.
//!
//! Version one intentionally exposes one narrow proof adapter. Module input selects immutable
//! source bytes and bounded literal attacker input; it cannot select a host command, environment,
//! network policy, evaluator, sandbox backend, or credential source.

use super::{ProductRunInput, dependency_preparation, scoped_source};
use crate::control::PermissionCapability;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_modules::Registry;
use peritus_plugin_sdk::{
    ModuleHostOutcome, ModuleJobCancelRequest, ModuleJobHandle, ModuleJobLimits, ModuleJobLogPage,
    ModuleJobLogRequest, ModuleJobPhase, ModuleJobReceipt, ModuleJobRequest, ModuleJobStatus,
    ModuleJobStatusRequest, ModuleJobStream, ModuleJobVerdict, ModulePatchReceipt,
    ModulePreparationReceipt, ScopedSourceAuthority,
};
use peritus_process::{
    CancellationReason, CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionPlan,
    GracefulAction, IoMode, NativeSandboxBackend, OsExitObservation, OutputOverflowAction,
    OutputPolicy, OutputStream, ProcessControl, ProcessResourcePolicy, StdinPolicy,
    TerminalDisposition, WorkingDirectory, WorkspaceAccess,
};
use peritus_sandbox::{
    AdmissionProfile, DescendantPolicy, EnvironmentContract, EnvironmentMode,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, InputPermission, IsolationRequirement, NetworkContract, PathScope,
    ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TreeContainment, admit_backend,
    compile_sandbox,
};
use peritus_sandbox_linux::{LinuxBackend, LinuxBackendConfig};
use peritus_types::{EventId, ResourceQuantity};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const SCHEMA: u16 = 1;
const PROOF_OPERATION: &str = "linux.canary-disclosure.v1";
const PATCHED_PROOF_OPERATION: &str = "linux.patched-canary-replay.v1";
const PREPARATION_OPERATION: &str = "https.dependency-fetch.v1";
const OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const PREPARATION_BYTES: u64 = 64 * 1024 * 1024;
const PATCH_SOURCE_BYTES: u64 = 1024 * 1024;
const PATCH_SOURCE_LENGTH: usize = 1024 * 1024;
const MAX_LOG_PAGE: u64 = 1024 * 1024;
const MAX_ACTIVE_JOBS: usize = 4;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CanaryDisclosureSpec {
    target_path: String,
    attacker_input: String,
    #[serde(default)]
    patch: Option<CopyPatchSpec>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CopyPatchSpec {
    expected_source_digest: String,
    replacement: String,
    case_id: String,
    expected_stdout_fragment: String,
    findings: Vec<String>,
    prerequisites: Vec<String>,
    order: u32,
    dependencies: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct JobIndex {
    schema: u16,
    jobs: BTreeMap<String, StoredJob>,
}

impl Default for JobIndex {
    fn default() -> Self {
        Self { schema: SCHEMA, jobs: BTreeMap::new() }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredJob {
    status: ModuleJobStatus,
    source_handle: String,
    scope_revision: String,
    snapshot_digest: String,
    target_digest: String,
    oracle_digest: String,
    execution_plan_digest: String,
    execution_run: [u8; 16],
    action: [u8; 16],
    process: [u8; 16],
    scratch: PathBuf,
    #[serde(default)]
    expected_stdout_fragment: Option<String>,
    #[serde(default)]
    patch: Option<ModulePatchReceipt>,
}

enum ActiveControl {
    Process(ProcessControl),
    Preparation(Arc<AtomicBool>),
}

struct ActiveJob {
    control: ActiveControl,
}

impl ActiveJob {
    fn process(&self) -> Option<ProcessControl> {
        match &self.control {
            ActiveControl::Process(control) => Some(control.clone()),
            ActiveControl::Preparation(_) => None,
        }
    }

    fn cancel(&self) -> Result<(), JobError> {
        match &self.control {
            ActiveControl::Process(control) => control
                .cancel(CancellationReason::User)
                .map_err(|error| failed("job_cancel", &error.to_string(), true)),
            ActiveControl::Preparation(cancelled) => {
                cancelled.store(true, Ordering::Release);
                Ok(())
            }
        }
    }
}

fn active() -> &'static Mutex<BTreeMap<String, ActiveJob>> {
    static ACTIVE: OnceLock<Mutex<BTreeMap<String, ActiveJob>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn state_lock() -> &'static Mutex<()> {
    static STATE: OnceLock<Mutex<()>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(()))
}

pub(super) struct Broker<'a> {
    input: &'a ProductRunInput,
    root: PathBuf,
    source_state: &'a Path,
    module_id: &'a str,
    module_digest: &'a str,
}

impl<'a> Broker<'a> {
    pub(super) fn new(
        input: &'a ProductRunInput,
        host_state: &'a Path,
        source_state: &'a Path,
        module_id: &'a str,
        module_digest: &'a str,
    ) -> Self {
        Self {
            input,
            root: host_state.join("owned-jobs-v1"),
            source_state,
            module_id,
            module_digest,
        }
    }

    pub(super) fn dispatch(&self, key: &str, operation: &str, input: Value) -> ModuleHostOutcome {
        let result = match operation {
            "submit" => decode(input).and_then(|request| self.submit(key, request)),
            "poll" => decode(input).and_then(|request| self.poll(request)),
            "log" => decode(input).and_then(|request| self.log(request)),
            "cancel" => decode(input).and_then(|request| self.cancel(request)),
            _ => {
                return ModuleHostOutcome::Unsupported {
                    detail: format!(
                        "owned-jobs operation {operation} is unavailable in contract version 1"
                    ),
                };
            }
        };
        match result {
            Ok(output) => ModuleHostOutcome::Success { output },
            Err(JobError::Rejected { code, detail }) => {
                ModuleHostOutcome::Rejected { code, detail }
            }
            Err(JobError::Failed { code, detail, retryable }) => {
                ModuleHostOutcome::Failed { code, detail, retryable_with_new_action: retryable }
            }
        }
    }

    fn submit(&self, key: &str, request: ModuleJobRequest) -> Result<Value, JobError> {
        self.require_effect_authority()?;
        let config = self.input.module_jobs.as_ref().ok_or_else(|| {
            rejected(
                "native_jobs_unavailable",
                "this host has no configured native Linux module-job backend",
            )
        })?;
        if request.schema != SCHEMA {
            return Err(rejected("invalid_job_request", "owned jobs require schema 1"));
        }
        if request.operation == PREPARATION_OPERATION {
            return self.submit_preparation(key, request);
        }
        if !matches!(request.operation.as_str(), PROOF_OPERATION | PATCHED_PROOF_OPERATION) {
            return Err(rejected(
                "invalid_job_request",
                "the requested stable operation adapter is unavailable",
            ));
        }
        let specification: CanaryDisclosureSpec = decode(request.specification.clone())?;
        let patched = request.operation == PATCHED_PROOF_OPERATION;
        if patched != specification.patch.is_some() {
            return Err(rejected(
                "invalid_job_request",
                "patched replay requires exactly one copy-only patch specification",
            ));
        }
        validate_target_path(&specification.target_path)?;
        if specification.attacker_input.len() > 4096
            || specification.attacker_input.contains('\0')
            || specification.attacker_input.chars().any(|character| character.is_control())
        {
            return Err(rejected(
                "invalid_attacker_input",
                "attacker input must be a bounded plain literal",
            ));
        }
        let limits = narrow_limits(&request.limits)?;
        let admitted = serde_json::to_vec(&(
            SCHEMA,
            key,
            self.module_digest,
            &request.source_handle,
            &request.scope_revision,
            request.operation.as_str(),
            &specification,
            &limits,
        ))
        .map_err(json_error)?;
        let request_digest = digest(&admitted);
        let job_id = digest(
            format!("peritus.module-job.v1\0{}\0{request_digest}", self.module_digest).as_bytes(),
        );
        let _state = state_lock().lock().map_err(|_| state_poisoned())?;
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let mut index = load_index(&self.root)?;
        if reconcile_orphans(&self.root, &mut index)? {
            save_index(&self.root, &index)?;
        }
        if let Some(existing) = index.jobs.get(&job_id) {
            return serde_json::to_value(ModuleJobHandle {
                schema: SCHEMA,
                job_id,
                source_handle: existing.source_handle.clone(),
                scope_revision: existing.scope_revision.clone(),
            })
            .map_err(json_error);
        }
        let active_count = index
            .jobs
            .values()
            .filter(|job| {
                matches!(job.status.phase, ModuleJobPhase::Queued | ModuleJobPhase::Running)
            })
            .count();
        if active_count >= MAX_ACTIVE_JOBS {
            return Err(rejected(
                "job_capacity",
                "this module already owns the maximum number of active jobs",
            ));
        }
        let directory = self.root.join("jobs").join(&job_id);
        if fs::symlink_metadata(&directory).is_ok() {
            return Err(failed(
                "orphan_job_state",
                "job storage exists without a durable index record; preserve it for diagnosis",
                false,
            ));
        }
        let scratch = directory.join("scratch");
        let work = scratch.join("work");
        let inputs = scratch.join("inputs");
        fs::create_dir_all(&work).map_err(io_error)?;
        fs::create_dir(&inputs).map_err(io_error)?;
        let authority = ScopedSourceAuthority {
            handle: request.source_handle.clone(),
            scope_revision: request.scope_revision.clone(),
        };
        let source = scoped_source::Broker::new(self.input, self.source_state, self.module_digest)
            .materialize_file(&authority, &specification.target_path, &inputs.join("original"))
            .map_err(source_error)?;
        let (target, patch, expected_stdout_fragment) = if let Some(patch) = &specification.patch {
            let (target, receipt) = prepare_copy_patch(
                &source,
                &inputs.join("target"),
                &specification.target_path,
                patch,
            )?;
            (target, Some(receipt), Some(patch.expected_stdout_fragment.clone()))
        } else {
            let target_path = inputs.join("target");
            fs::rename(&source.path, &target_path).map_err(io_error)?;
            (
                scoped_source::MaterializedSourceFile {
                    snapshot_digest: source.snapshot_digest,
                    path: target_path,
                    digest: source.digest,
                },
                None,
                None,
            )
        };
        let canary = random_canary()?;
        let canary_path = inputs.join("canary");
        write_private(&canary_path, &canary)?;
        let oracle_digest = digest(
            [b"contains-exact-random-canary-v1\0".as_slice(), Sha256::digest(&canary).as_slice()]
                .concat()
                .as_slice(),
        );
        let context = self
            .input
            .command_runtime
            .reserve_native_module_job_context(self.input.run_id)
            .map_err(runtime_error)?;
        let identity = context.identity();
        let prepared = prepare(
            config,
            identity,
            &work,
            &target.path,
            &canary_path,
            &specification.attacker_input,
            &limits,
        )?;
        let plan_digest = hex(prepared.plan.digest().as_bytes());
        let status = ModuleJobStatus {
            schema: SCHEMA,
            job_id: job_id.clone(),
            phase: ModuleJobPhase::Queued,
            operation: request.operation.clone(),
            request_digest: request_digest.clone(),
            limits: limits.clone(),
            receipt: None,
            detail: "accepted; native launch has not completed".into(),
        };
        index.jobs.insert(
            job_id.clone(),
            StoredJob {
                status,
                source_handle: request.source_handle.clone(),
                scope_revision: request.scope_revision.clone(),
                snapshot_digest: target.snapshot_digest,
                target_digest: target.digest,
                oracle_digest,
                execution_plan_digest: plan_digest,
                execution_run: identity.run_id().into_bytes(),
                action: identity.action_id().into_bytes(),
                process: identity.process_id().into_bytes(),
                scratch: scratch.clone(),
                expected_stdout_fragment,
                patch,
            },
        );
        save_index(&self.root, &index)?;
        let process = match self.input.command_runtime.launch_native_module_job(
            context,
            prepared.plan,
            &prepared.sandbox,
            &prepared.admission,
            prepared.backend,
        ) {
            Ok(process) => process,
            Err(error) => {
                let job = index.jobs.get_mut(&job_id).expect("inserted job");
                job.status.phase = ModuleJobPhase::Failed;
                job.status.detail =
                    format!("native launch failed before target execution: {error}");
                save_index(&self.root, &index)?;
                return serde_json::to_value(ModuleJobHandle {
                    schema: SCHEMA,
                    job_id,
                    source_handle: request.source_handle,
                    scope_revision: request.scope_revision,
                })
                .map_err(json_error);
            }
        };
        let control = process.control();
        index.jobs.get_mut(&job_id).expect("inserted job").status.phase = ModuleJobPhase::Running;
        index.jobs.get_mut(&job_id).expect("inserted job").status.detail =
            "running in the native Linux private-filesystem backend with network and credentials denied"
                .into();
        save_index(&self.root, &index)?;
        let active_key = self.active_key(&job_id);
        active().lock().map_err(|_| state_poisoned())?.insert(
            active_key.clone(),
            ActiveJob { control: ActiveControl::Process(control.clone()) },
        );
        let owner = launch_owner(OwnerInput {
            root: self.root.clone(),
            active_key: active_key.clone(),
            job_id: job_id.clone(),
            process,
            control,
            canary,
            expected_stdout_fragment: index
                .jobs
                .get(&job_id)
                .and_then(|job| job.expected_stdout_fragment.clone()),
            source_state: self.source_state.to_path_buf(),
            authority,
            module_id: self.module_id.to_owned(),
            module_digest: self.module_digest.to_owned(),
            cancelled: Arc::clone(&self.input.cancelled),
            provider_cancellation: self.input.provider_cancellation.clone(),
            conversation: Arc::clone(&self.input.conversation),
        });
        if let Err(error) = owner {
            active().lock().map_err(|_| state_poisoned())?.remove(&active_key);
            let job = index.jobs.get_mut(&job_id).expect("inserted job");
            job.status.phase = ModuleJobPhase::Indeterminate;
            job.status.detail = format!(
                "native launch succeeded but its owner thread could not start; process drop requested cancellation: {error:?}"
            );
            save_index(&self.root, &index)?;
        }
        serde_json::to_value(ModuleJobHandle {
            schema: SCHEMA,
            job_id,
            source_handle: request.source_handle,
            scope_revision: request.scope_revision,
        })
        .map_err(json_error)
    }

    fn submit_preparation(&self, key: &str, request: ModuleJobRequest) -> Result<Value, JobError> {
        let specification: dependency_preparation::Specification =
            decode(request.specification.clone())?;
        let limits = narrow_limits(&request.limits)?;
        dependency_preparation::validate(&specification, limits.storage_bytes)
            .map_err(|detail| rejected("invalid_dependency_preparation", &detail))?;
        let authority = ScopedSourceAuthority {
            handle: request.source_handle.clone(),
            scope_revision: request.scope_revision.clone(),
        };
        let snapshot_digest =
            scoped_source::Broker::new(self.input, self.source_state, self.module_digest)
                .validate_authority(&authority)
                .map_err(source_error)?;
        let admitted = serde_json::to_vec(&(
            SCHEMA,
            key,
            self.module_digest,
            &request.source_handle,
            &request.scope_revision,
            PREPARATION_OPERATION,
            &specification,
            &limits,
        ))
        .map_err(json_error)?;
        let request_digest = digest(&admitted);
        let job_id = digest(
            format!("peritus.module-job.v1\0{}\0{request_digest}", self.module_digest).as_bytes(),
        );
        let _state = state_lock().lock().map_err(|_| state_poisoned())?;
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let mut index = load_index(&self.root)?;
        if reconcile_orphans(&self.root, &mut index)? {
            save_index(&self.root, &index)?;
        }
        if let Some(existing) = index.jobs.get(&job_id) {
            return job_handle(
                job_id,
                existing.source_handle.clone(),
                existing.scope_revision.clone(),
            );
        }
        enforce_capacity(&index)?;
        let directory = self.root.join("jobs").join(&job_id);
        if fs::symlink_metadata(&directory).is_ok() {
            return Err(failed(
                "orphan_job_state",
                "job storage exists without a durable index record; preserve it for diagnosis",
                false,
            ));
        }
        let scratch = directory.join("scratch");
        fs::create_dir_all(&scratch).map_err(io_error)?;
        let execution_plan_digest = digest(&admitted);
        let oracle_digest = digest(
            format!(
                "exact-sha256-v1\0{}\0{}",
                specification.expected_sha256.as_deref().unwrap_or_default(),
                specification.maximum_bytes
            )
            .as_bytes(),
        );
        let status = ModuleJobStatus {
            schema: SCHEMA,
            job_id: job_id.clone(),
            phase: ModuleJobPhase::Running,
            operation: PREPARATION_OPERATION.into(),
            request_digest: request_digest.clone(),
            limits: limits.clone(),
            receipt: None,
            detail: "retrieving one declared HTTPS dependency through a credential-free, public-address-only host adapter".into(),
        };
        index.jobs.insert(
            job_id.clone(),
            StoredJob {
                status,
                source_handle: request.source_handle.clone(),
                scope_revision: request.scope_revision.clone(),
                snapshot_digest,
                target_digest: specification.expected_sha256.clone().unwrap_or_default(),
                oracle_digest,
                execution_plan_digest,
                execution_run: [0; 16],
                action: [0; 16],
                process: [0; 16],
                scratch,
                expected_stdout_fragment: None,
                patch: None,
            },
        );
        save_index(&self.root, &index)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let active_key = self.active_key(&job_id);
        active().lock().map_err(|_| state_poisoned())?.insert(
            active_key.clone(),
            ActiveJob { control: ActiveControl::Preparation(Arc::clone(&cancelled)) },
        );
        let owner = launch_preparation_owner(PreparationOwnerInput {
            root: self.root.clone(),
            active_key: active_key.clone(),
            job_id: job_id.clone(),
            specification,
            cancellation: cancelled,
            source_state: self.source_state.to_path_buf(),
            authority,
            module_id: self.module_id.to_owned(),
            module_digest: self.module_digest.to_owned(),
            run_cancelled: Arc::clone(&self.input.cancelled),
            provider_cancellation: self.input.provider_cancellation.clone(),
            conversation: Arc::clone(&self.input.conversation),
        });
        if let Err(error) = owner {
            active().lock().map_err(|_| state_poisoned())?.remove(&active_key);
            let job = index.jobs.get_mut(&job_id).expect("inserted job");
            let cleanup = fs::remove_dir_all(&job.scratch).is_ok();
            job.status.phase = ModuleJobPhase::Failed;
            job.status.detail = format!(
                "dependency preparation owner could not start: {error:?}; cleanup complete: {cleanup}"
            );
            save_index(&self.root, &index)?;
        }
        job_handle(job_id, request.source_handle, request.scope_revision)
    }

    fn poll(&self, request: ModuleJobStatusRequest) -> Result<Value, JobError> {
        self.require_read_authority()?;
        if request.schema != SCHEMA || !valid_job_id(&request.job_id) {
            return Err(rejected(
                "invalid_job_poll",
                "job polling requires schema 1 and an exact job identity",
            ));
        }
        let wait = request.wait_millis.min(1_000);
        let began = Instant::now();
        loop {
            let state_guard = state_lock().lock().map_err(|_| state_poisoned())?;
            let mut index = load_index(&self.root)?;
            let status = index
                .jobs
                .get(&request.job_id)
                .ok_or_else(|| rejected("unknown_job", "the job is not retained by this module"))?
                .status
                .clone();
            if !matches!(status.phase, ModuleJobPhase::Queued | ModuleJobPhase::Running) {
                return serde_json::to_value(status).map_err(json_error);
            }
            let active_key = self.active_key(&request.job_id);
            let live = active().lock().map_err(|_| state_poisoned())?.contains_key(&active_key);
            if !live {
                let job = index.jobs.get_mut(&request.job_id).expect("observed job");
                job.status.phase = ModuleJobPhase::Indeterminate;
                job.status.detail = "the durable job was nonterminal but its in-memory owner is absent; no side effect was relaunched".into();
                let status = job.status.clone();
                save_index(&self.root, &index)?;
                return serde_json::to_value(status).map_err(json_error);
            }
            if began.elapsed() >= Duration::from_millis(wait) {
                return serde_json::to_value(status).map_err(json_error);
            }
            drop(state_guard);
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn log(&self, request: ModuleJobLogRequest) -> Result<Value, JobError> {
        self.require_read_authority()?;
        if request.schema != SCHEMA
            || !valid_job_id(&request.job_id)
            || request.maximum_bytes == 0
            || request.maximum_bytes > MAX_LOG_PAGE
        {
            return Err(rejected(
                "invalid_job_log",
                "job logs require schema 1, an exact job identity, and 1..=1048576 bytes",
            ));
        }
        let index = load_index_locked(&self.root)?;
        let job = index
            .jobs
            .get(&request.job_id)
            .ok_or_else(|| rejected("unknown_job", "the job is not retained by this module"))?;
        let maximum = usize::try_from(request.maximum_bytes)
            .map_err(|_| rejected("invalid_job_log", "log range exceeds this platform"))?;
        let (total, bytes) = match request.stream {
            ModuleJobStream::Controller => {
                page(job.status.detail.as_bytes(), request.offset, maximum)?
            }
            ModuleJobStream::Stdout | ModuleJobStream::Stderr => {
                let active_key = self.active_key(&request.job_id);
                let control = active()
                    .lock()
                    .map_err(|_| state_poisoned())?
                    .get(&active_key)
                    .and_then(ActiveJob::process);
                if let Some(control) = control {
                    control
                        .spooled_stream_range(
                            match request.stream {
                                ModuleJobStream::Stdout => OutputStream::Stdout,
                                ModuleJobStream::Stderr => OutputStream::Stderr,
                                ModuleJobStream::Controller => unreachable!(),
                            },
                            request.offset,
                            maximum,
                        )
                        .map_err(|error| failed("job_log", &error.to_string(), true))?
                } else {
                    let name = match request.stream {
                        ModuleJobStream::Stdout => "stdout.log",
                        ModuleJobStream::Stderr => "stderr.log",
                        ModuleJobStream::Controller => unreachable!(),
                    };
                    let bytes = fs::read(self.root.join("jobs").join(&request.job_id).join(name))
                        .unwrap_or_default();
                    page(&bytes, request.offset, maximum)?
                }
            }
        };
        let end = request
            .offset
            .checked_add(u64::try_from(bytes.len()).map_err(|_| representation())?)
            .ok_or_else(representation)?;
        serde_json::to_value(ModuleJobLogPage {
            schema: SCHEMA,
            job_id: request.job_id,
            stream: request.stream,
            range_start: request.offset,
            range_end: end,
            total_bytes: total,
            bytes,
        })
        .map_err(json_error)
    }

    fn cancel(&self, request: ModuleJobCancelRequest) -> Result<Value, JobError> {
        self.require_effect_authority()?;
        if request.schema != SCHEMA
            || !valid_job_id(&request.job_id)
            || request.reason.trim().is_empty()
            || request.reason.len() > 1024
            || request.reason.chars().any(char::is_control)
        {
            return Err(rejected(
                "invalid_job_cancel",
                "job cancellation requires schema 1, an exact job identity, and a plain reason",
            ));
        }
        let _state = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load_index(&self.root)?;
        let job = index
            .jobs
            .get_mut(&request.job_id)
            .ok_or_else(|| rejected("unknown_job", "the job is not retained by this module"))?;
        if !matches!(job.status.phase, ModuleJobPhase::Queued | ModuleJobPhase::Running) {
            return serde_json::to_value(&job.status).map_err(json_error);
        }
        let active_key = self.active_key(&request.job_id);
        let cancelled =
            active().lock().map_err(|_| state_poisoned())?.get(&active_key).map(ActiveJob::cancel);
        let Some(cancelled) = cancelled else {
            job.status.phase = ModuleJobPhase::Indeterminate;
            job.status.detail =
                "cancellation found no exact live owner; no process was guessed or relaunched"
                    .into();
            let status = job.status.clone();
            save_index(&self.root, &index)?;
            return serde_json::to_value(status).map_err(json_error);
        };
        cancelled?;
        job.status.detail = format!("cancellation requested: {}", request.reason);
        let status = job.status.clone();
        save_index(&self.root, &index)?;
        serde_json::to_value(status).map_err(json_error)
    }

    fn require_read_authority(&self) -> Result<(), JobError> {
        if self.input.conversation.effective_permissions().allows(PermissionCapability::Read) {
            Ok(())
        } else {
            Err(rejected(
                "job_read_permission",
                "current conversation authority does not permit job observation",
            ))
        }
    }

    fn require_effect_authority(&self) -> Result<(), JobError> {
        let permissions = self.input.conversation.effective_permissions();
        if permissions.allows(PermissionCapability::Read)
            && permissions.allows(PermissionCapability::Write)
            && permissions.allows(PermissionCapability::Process)
        {
            Ok(())
        } else {
            Err(rejected(
                "job_effect_permission",
                "current conversation authority does not permit read, write, and process effects",
            ))
        }
    }

    fn active_key(&self, job_id: &str) -> String {
        active_key(&self.root, job_id)
    }
}

struct Prepared {
    plan: ExecutionPlan,
    sandbox: peritus_sandbox::CheckedSandboxPlan,
    admission: peritus_sandbox::BackendAdmission,
    backend: LinuxBackend,
}

fn prepare(
    config: &crate::NativeModuleJobConfiguration,
    identity: peritus_process::ExecutionIdentity,
    work: &Path,
    target: &Path,
    canary: &Path,
    attacker_input: &str,
    limits: &ModuleJobLimits,
) -> Result<Prepared, JobError> {
    // The Linux backend always protects repository-control metadata beneath its workspace root.
    // This job workspace is fresh and private, so materialize the empty roots up front: an absent
    // protected root underneath a descendant-creation grant is intentionally not representable
    // because the target could create it after the mount plan was frozen.
    for protected in [".git", ".peritus", ".crosslink"] {
        fs::create_dir_all(work.join(protected)).map_err(io_error)?;
    }
    let shell = fs::canonicalize("/bin/sh").map_err(|_| {
        rejected("job_runtime", "the host-owned /bin/sh interpreter is unavailable")
    })?;
    let resources = ProcessResourcePolicy::new(
        Some(limits.wall_millis),
        Some(limits.cpu_millis),
        limits.memory_bytes,
        limits.storage_bytes,
        Some(OUTPUT_BYTES),
        u64::from(limits.processes),
        128,
        1,
    )
    .map_err(process_error)?;
    let backend = LinuxBackend::new(
        LinuxBackendConfig::new(
            work.to_path_buf(),
            Vec::new(),
            config.bubblewrap.clone(),
            config.linux_helper.clone(),
            config.cgroup_root.clone(),
            None,
        )
        .map_err(linux_error)?
        .with_private_filesystem(),
    )
    .map_err(linux_error)?;
    let sandbox = compile(identity, work, target, canary, &shell, resources)?;
    let admission = admit_backend(&sandbox, backend.descriptor(), AdmissionProfile::Production)
        .map_err(sandbox_error)?;
    let working = WorkingDirectory::open(
        work,
        identity.workspace_id(),
        identity.resource_id(),
        identity.environment_id(),
        identity.revision().workspace_generation(),
        identity.revision().workspace_revision(),
        WorkspaceAccess::Writable,
    )
    .map_err(process_error)?;
    let output = OutputPolicy::new(
        4096,
        64 * 1024,
        OUTPUT_BYTES,
        8192,
        OUTPUT_BYTES,
        OUTPUT_BYTES,
        OUTPUT_BYTES,
        OutputOverflowAction::Terminate,
    )
    .map_err(process_error)?;
    let command = CommandSpec::new(
        shell.to_str().ok_or_else(|| rejected("job_runtime", "shell path is not UTF-8"))?,
        vec![
            target.to_string_lossy().into_owned(),
            canary.to_string_lossy().into_owned(),
            attacker_input.to_owned(),
        ],
    )
    .map_err(process_error)?;
    let plan = ExecutionPlan::new(
        identity,
        command,
        working,
        EnvironmentPlan::cleared(Vec::new()).map_err(process_error)?,
        IoMode::Pipes,
        StdinPolicy::Closed,
        output,
        DeadlinePolicy::new(Some(limits.wall_millis), GracefulAction::Terminate, 100, 2_000)
            .map_err(process_error)?,
        resources,
        &sandbox,
        &admission,
    )
    .map_err(process_error)?;
    Ok(Prepared { plan, sandbox, admission, backend })
}

fn compile(
    identity: peritus_process::ExecutionIdentity,
    work: &Path,
    target: &Path,
    canary: &Path,
    shell: &Path,
    resources: ProcessResourcePolicy,
) -> Result<peritus_sandbox::CheckedSandboxPlan, JobError> {
    let executable = sandbox_path(shell)?;
    let limits = ResourceLimits::with_optional_wall_output_cpu(
        resources.wall_millis().map(ResourceQuantity::new),
        resources.output_bytes().map(ResourceQuantity::new),
        resources.cpu_millis().map(ResourceQuantity::new),
        ResourceQuantity::new(resources.memory_bytes()),
        ResourceQuantity::new(resources.disk_bytes()),
        ResourceQuantity::new(resources.file_descriptors()),
        ResourceQuantity::new(resources.process_count()),
        ResourceQuantity::new(resources.concurrent_slots()),
    )
    .map_err(sandbox_error)?;
    let output = ResourceQuantity::new(OUTPUT_BYTES);
    let events = ResourceQuantity::new(8192);
    let descendants =
        u32::try_from(resources.process_count().saturating_sub(1)).map_err(|_| representation())?;
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Denied,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        TerminalLimits::new(None, events, output).map_err(sandbox_error)?,
    )
    .map_err(sandbox_error)?;
    let filesystem = filesystem(work, target, canary, shell)?;
    let contract = SandboxContract::new(
        filesystem,
        ProcessContract::new(
            vec![executable.clone()],
            DescendantPolicy::Bounded(descendants),
            SignalPolicy::GracefulAndForced,
            TreeContainment::Required,
            descendants.saturating_add(1),
        )
        .map_err(sandbox_error)?,
        EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new()).map_err(sandbox_error)?,
        NetworkContract::deny_all(),
        SecretContract::deny_all(),
        limits,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![
            FileRequirement::new(executable.clone(), FileOperation::Execute),
            FileRequirement::new(sandbox_path(target)?, FileOperation::Read),
            FileRequirement::new(sandbox_path(canary)?, FileOperation::Read),
        ],
        ProcessRequirements::new(executable, descendants, true),
        EnvironmentRequirements::new(Vec::new(), Vec::new()).map_err(sandbox_error)?,
        Vec::new(),
        Vec::new(),
        limits,
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Allowed,
            None,
            events,
            output,
        )
        .map_err(sandbox_error)?,
    )
    .map_err(sandbox_error)?;
    compile_sandbox(
        SandboxBinding::new(
            identity.process_id(),
            identity.resource_id(),
            identity.environment_id(),
            identity.revision(),
        ),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .map_err(sandbox_error)
}

fn filesystem(
    work: &Path,
    target: &Path,
    canary: &Path,
    shell: &Path,
) -> Result<FilesystemContract, JobError> {
    let read = FileOperationSet::from_operations([FileOperation::Read, FileOperation::Metadata]);
    let execute = FileOperationSet::from_operations([
        FileOperation::Read,
        FileOperation::Metadata,
        FileOperation::Execute,
    ]);
    let mut rules = vec![
        FilesystemRule::new(RuleEffect::Allow, sandbox_path(shell)?, PathScope::Exact, execute)
            .map_err(sandbox_error)?,
        FilesystemRule::new(RuleEffect::Allow, sandbox_path(target)?, PathScope::Exact, read)
            .map_err(sandbox_error)?,
        FilesystemRule::new(RuleEffect::Allow, sandbox_path(canary)?, PathScope::Exact, read)
            .map_err(sandbox_error)?,
        FilesystemRule::new(
            RuleEffect::Allow,
            sandbox_path(work)?,
            PathScope::Descendants,
            FileOperationSet::from_operations([
                FileOperation::Read,
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Create,
                FileOperation::Write,
                FileOperation::Remove,
            ]),
        )
        .map_err(sandbox_error)?,
    ];
    for runtime in ["/usr/lib", "/usr/lib64", "/lib", "/lib64"] {
        let Ok(runtime) = Path::new(runtime).canonicalize() else { continue };
        rules.push(
            FilesystemRule::new(
                RuleEffect::Allow,
                sandbox_path(&runtime)?,
                PathScope::Descendants,
                execute,
            )
            .map_err(sandbox_error)?,
        );
    }
    FilesystemContract::new(rules).map_err(sandbox_error)
}

struct OwnerInput {
    root: PathBuf,
    active_key: String,
    job_id: String,
    process: peritus_process::OwnedProcess,
    control: ProcessControl,
    canary: Vec<u8>,
    expected_stdout_fragment: Option<String>,
    source_state: PathBuf,
    authority: ScopedSourceAuthority,
    module_id: String,
    module_digest: String,
    cancelled: Arc<AtomicBool>,
    provider_cancellation: peritus_provider_core::CancellationToken,
    conversation: Arc<dyn crate::ConversationView>,
}

fn launch_owner(input: OwnerInput) -> Result<(), JobError> {
    thread::Builder::new()
        .name(format!("peritus-module-job-{}", &input.job_id[..12]))
        .spawn(move || finish_owner(input))
        .map(|_| ())
        .map_err(|error| failed("job_owner", &error.to_string(), true))
}

fn finish_owner(input: OwnerInput) {
    let monitor_control = input.control.clone();
    let cancelled = Arc::clone(&input.cancelled);
    let provider_cancellation = input.provider_cancellation.clone();
    let conversation = Arc::clone(&input.conversation);
    let source_state = input.source_state.clone();
    let authority = input.authority.clone();
    let module_id = input.module_id.clone();
    let module_digest = input.module_digest.clone();
    let done = Arc::new(AtomicBool::new(false));
    let monitor_done = Arc::clone(&done);
    let monitor = thread::spawn(move || {
        while !monitor_done.load(Ordering::Acquire) {
            let permissions = conversation.effective_permissions();
            if cancelled.load(Ordering::Acquire)
                || provider_cancellation.is_cancelled()
                || !permissions.allows(PermissionCapability::Read)
                || !permissions.allows(PermissionCapability::Write)
                || !permissions.allows(PermissionCapability::Process)
                || !scoped_source::authority_live(&source_state, &authority)
                || !module_live(&module_id, &module_digest)
            {
                let _ = monitor_control.cancel(CancellationReason::User);
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    let store = StoreConfig::new(input.root.join("artifacts"), OUTPUT_BYTES, 256 * 1024 * 1024)
        .and_then(ArtifactStore::open);
    let result = match store {
        Ok(store) => input
            .process
            .wait_and_publish(&store, event_id(&input.job_id))
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    done.store(true, Ordering::Release);
    let _ = monitor.join();
    let stdout = input
        .control
        .spooled_stream_range(OutputStream::Stdout, 0, OUTPUT_BYTES as usize)
        .map(|(_, bytes)| bytes)
        .unwrap_or_default();
    let stderr = input
        .control
        .spooled_stream_range(OutputStream::Stderr, 0, OUTPUT_BYTES as usize)
        .map(|(_, bytes)| bytes)
        .unwrap_or_default();
    let directory = input.root.join("jobs").join(&input.job_id);
    let _ = publish_bytes(&directory.join("stdout.log"), &stdout);
    let _ = publish_bytes(&directory.join("stderr.log"), &stderr);
    if let Ok(_guard) = state_lock().lock() {
        if let Ok(mut index) = load_index(&input.root) {
            if let Some(job) = index.jobs.get_mut(&input.job_id) {
                match result {
                    Ok(terminal) => {
                        let cleanup = terminal.tree_cleanup_complete()
                            && terminal.support_tasks_joined()
                            && fs::remove_dir_all(&job.scratch).is_ok();
                        let clean_exit =
                            matches!(terminal.disposition(), TerminalDisposition::Exited)
                                && matches!(terminal.os_exit(), OsExitObservation::Code(0));
                        let leaked = contains(&stdout, &input.canary);
                        let expected_output_observed = input
                            .expected_stdout_fragment
                            .as_ref()
                            .is_none_or(|expected| contains(&stdout, expected.as_bytes()));
                        let verdict = if clean_exit && leaked {
                            ModuleJobVerdict::ViolationObserved
                        } else if clean_exit && expected_output_observed {
                            ModuleJobVerdict::PropertyHeldForCase
                        } else {
                            ModuleJobVerdict::Indeterminate
                        };
                        let receipt = ModuleJobReceipt {
                            schema: SCHEMA,
                            job_id: input.job_id.clone(),
                            operation: job.status.operation.clone(),
                            request_digest: job.status.request_digest.clone(),
                            module_digest: input.module_digest,
                            source_handle: job.source_handle.clone(),
                            scope_revision: job.scope_revision.clone(),
                            snapshot_digest: job.snapshot_digest.clone(),
                            target_digest: job.target_digest.clone(),
                            backend: format!(
                                "{}@{}",
                                peritus_sandbox_linux::BACKEND_NAME,
                                peritus_sandbox_linux::BACKEND_VERSION
                            ),
                            execution_plan_digest: job.execution_plan_digest.clone(),
                            oracle_digest: job.oracle_digest.clone(),
                            verdict,
                            terminal_disposition: format!("{:?}", terminal.disposition())
                                .to_ascii_lowercase(),
                            exit_code: match terminal.os_exit() {
                                OsExitObservation::Code(code) => Some(*code),
                                _ => None,
                            },
                            stdout_artifact: digest(&stdout),
                            stderr_artifact: digest(&stderr),
                            limits: job.status.limits.clone(),
                            cleanup_complete: cleanup,
                            network_policy: "deny_all".into(),
                            preparation: None,
                            patch: job.patch.clone(),
                        };
                        job.status.phase = match terminal.disposition() {
                            TerminalDisposition::Cancelled => ModuleJobPhase::Cancelled,
                            TerminalDisposition::Exited if cleanup => ModuleJobPhase::Completed,
                            TerminalDisposition::RecoveryIndeterminate => {
                                ModuleJobPhase::Indeterminate
                            }
                            _ => ModuleJobPhase::Failed,
                        };
                        job.status.detail = format!(
                            "native owner reached {:?}; oracle {:?}; expected output observed: {}; cleanup complete: {}",
                            terminal.disposition(),
                            verdict,
                            expected_output_observed,
                            cleanup
                        );
                        job.status.receipt = Some(receipt);
                    }
                    Err(detail) => {
                        job.status.phase = ModuleJobPhase::Indeterminate;
                        job.status.detail = format!(
                            "native owner could not publish a complete terminal receipt: {detail}"
                        );
                    }
                }
                let _ = save_index(&input.root, &index);
            }
        }
    }
    if let Ok(mut active) = active().lock() {
        active.remove(&input.active_key);
    }
}

struct PreparationOwnerInput {
    root: PathBuf,
    active_key: String,
    job_id: String,
    specification: dependency_preparation::Specification,
    cancellation: Arc<AtomicBool>,
    source_state: PathBuf,
    authority: ScopedSourceAuthority,
    module_id: String,
    module_digest: String,
    run_cancelled: Arc<AtomicBool>,
    provider_cancellation: peritus_provider_core::CancellationToken,
    conversation: Arc<dyn crate::ConversationView>,
}

fn launch_preparation_owner(input: PreparationOwnerInput) -> Result<(), JobError> {
    thread::Builder::new()
        .name(format!("peritus-module-preparation-{}", &input.job_id[..12]))
        .spawn(move || finish_preparation_owner(input))
        .map(|_| ())
        .map_err(|error| failed("job_owner", &error.to_string(), true))
}

fn finish_preparation_owner(input: PreparationOwnerInput) {
    let done = Arc::new(AtomicBool::new(false));
    let monitor_done = Arc::clone(&done);
    let monitor_cancellation = Arc::clone(&input.cancellation);
    let run_cancelled = Arc::clone(&input.run_cancelled);
    let provider_cancellation = input.provider_cancellation.clone();
    let conversation = Arc::clone(&input.conversation);
    let source_state = input.source_state.clone();
    let authority = input.authority.clone();
    let module_id = input.module_id.clone();
    let module_digest = input.module_digest.clone();
    let monitor = thread::spawn(move || {
        while !monitor_done.load(Ordering::Acquire) {
            let permissions = conversation.effective_permissions();
            if run_cancelled.load(Ordering::Acquire)
                || provider_cancellation.is_cancelled()
                || !permissions.allows(PermissionCapability::Read)
                || !permissions.allows(PermissionCapability::Write)
                || !permissions.allows(PermissionCapability::Process)
                || !scoped_source::authority_live(&source_state, &authority)
                || !module_live(&module_id, &module_digest)
            {
                monitor_cancellation.store(true, Ordering::Release);
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    let result = dependency_preparation::fetch(&input.specification, &input.cancellation);
    done.store(true, Ordering::Release);
    let _ = monitor.join();
    if let Ok(_guard) = state_lock().lock() {
        if let Ok(mut index) = load_index(&input.root) {
            if let Some(job) = index.jobs.get_mut(&input.job_id) {
                finish_preparation_record(job, &input, result);
                let _ = save_index(&input.root, &index);
            }
        }
    }
    if let Ok(mut active) = active().lock() {
        active.remove(&input.active_key);
    }
}

fn finish_preparation_record(
    job: &mut StoredJob,
    input: &PreparationOwnerInput,
    result: Result<dependency_preparation::Completed, dependency_preparation::Failure>,
) {
    match result {
        Ok(completed) => {
            let store = StoreConfig::new(
                input.root.join("artifacts"),
                PREPARATION_BYTES,
                512 * 1024 * 1024,
            )
            .and_then(ArtifactStore::open);
            let artifact = store
                .map_err(|error| error.to_string())
                .and_then(|store| publish_dependency(&store, &completed.bytes));
            match artifact {
                Ok(artifact_digest) => {
                    let preparation = ModulePreparationReceipt {
                        requested_url: completed.requested_url,
                        final_url: completed.final_url,
                        redirects: completed.redirects,
                        resolved_addresses: completed.resolved_addresses,
                        bytes: u64::try_from(completed.bytes.len()).unwrap_or(u64::MAX),
                        artifact_digest,
                    };
                    let stdout = serde_json::to_vec_pretty(&preparation).unwrap_or_default();
                    let directory = input.root.join("jobs").join(&input.job_id);
                    let logs_published = publish_bytes(&directory.join("stdout.log"), &stdout)
                        .and_then(|()| publish_bytes(&directory.join("stderr.log"), &[]))
                        .is_ok();
                    let cleanup = fs::remove_dir_all(&job.scratch).is_ok() && logs_published;
                    let verdict = if cleanup {
                        ModuleJobVerdict::PropertyHeldForCase
                    } else {
                        ModuleJobVerdict::Indeterminate
                    };
                    job.status.receipt = Some(ModuleJobReceipt {
                        schema: SCHEMA,
                        job_id: input.job_id.clone(),
                        operation: PREPARATION_OPERATION.into(),
                        request_digest: job.status.request_digest.clone(),
                        module_digest: input.module_digest.clone(),
                        source_handle: job.source_handle.clone(),
                        scope_revision: job.scope_revision.clone(),
                        snapshot_digest: job.snapshot_digest.clone(),
                        target_digest: completed.digest,
                        backend: "peritus-host-https@1".into(),
                        execution_plan_digest: job.execution_plan_digest.clone(),
                        oracle_digest: job.oracle_digest.clone(),
                        verdict,
                        terminal_disposition: "completed".into(),
                        exit_code: None,
                        stdout_artifact: digest(&stdout),
                        stderr_artifact: digest(&[]),
                        limits: job.status.limits.clone(),
                        cleanup_complete: cleanup,
                        network_policy: "declared_https_public_only_then_disconnected".into(),
                        preparation: Some(preparation),
                        patch: None,
                    });
                    job.status.phase =
                        if cleanup { ModuleJobPhase::Completed } else { ModuleJobPhase::Failed };
                    job.status.detail = format!(
                        "dependency digest matched; preparation artifact retained; cleanup complete: {cleanup}"
                    );
                }
                Err(detail) => {
                    preparation_failed(
                        job,
                        input,
                        format!("artifact publication failed: {detail}"),
                    );
                }
            }
        }
        Err(dependency_preparation::Failure::Cancelled) => {
            let cleanup = fs::remove_dir_all(&job.scratch).is_ok();
            job.status.phase =
                if cleanup { ModuleJobPhase::Cancelled } else { ModuleJobPhase::Failed };
            job.status.detail =
                format!("dependency preparation was cancelled; cleanup complete: {cleanup}");
        }
        Err(dependency_preparation::Failure::Rejected(detail)) => {
            preparation_failed(job, input, format!("dependency rejected: {detail}"));
        }
        Err(dependency_preparation::Failure::Infrastructure(detail)) => {
            preparation_failed(job, input, format!("dependency infrastructure failed: {detail}"));
        }
    }
}

fn preparation_failed(job: &mut StoredJob, input: &PreparationOwnerInput, detail: String) {
    let directory = input.root.join("jobs").join(&input.job_id);
    let _ = publish_bytes(&directory.join("stdout.log"), &[]);
    let _ = publish_bytes(&directory.join("stderr.log"), detail.as_bytes());
    let cleanup = fs::remove_dir_all(&job.scratch).is_ok();
    job.status.phase = ModuleJobPhase::Failed;
    job.status.detail = format!("{detail}; cleanup complete: {cleanup}");
}

fn narrow_limits(requested: &ModuleJobLimits) -> Result<ModuleJobLimits, JobError> {
    if requested.memory_bytes == 0
        || requested.storage_bytes == 0
        || requested.cpu_millis == 0
        || requested.wall_millis == 0
        || requested.processes == 0
    {
        return Err(rejected("invalid_job_limits", "every job resource limit must be nonzero"));
    }
    Ok(ModuleJobLimits {
        memory_bytes: requested.memory_bytes.min(1024 * 1024 * 1024),
        storage_bytes: requested.storage_bytes.min(512 * 1024 * 1024),
        cpu_millis: requested.cpu_millis.min(3_600_000),
        wall_millis: requested.wall_millis.min(3_600_000),
        processes: requested.processes.min(32),
    })
}

fn enforce_capacity(index: &JobIndex) -> Result<(), JobError> {
    let active_count = index
        .jobs
        .values()
        .filter(|job| matches!(job.status.phase, ModuleJobPhase::Queued | ModuleJobPhase::Running))
        .count();
    if active_count >= MAX_ACTIVE_JOBS {
        Err(rejected("job_capacity", "this module already owns the maximum number of active jobs"))
    } else {
        Ok(())
    }
}

fn reconcile_orphans(root: &Path, index: &mut JobIndex) -> Result<bool, JobError> {
    let live = active().lock().map_err(|_| state_poisoned())?;
    let mut changed = false;
    for (job_id, job) in &mut index.jobs {
        if matches!(job.status.phase, ModuleJobPhase::Queued | ModuleJobPhase::Running)
            && !live.contains_key(&active_key(root, job_id))
        {
            job.status.phase = ModuleJobPhase::Indeterminate;
            job.status.detail = "startup reconciliation found a nonterminal durable job without its exact in-memory owner; no side effect was relaunched".into();
            changed = true;
        }
    }
    Ok(changed)
}

fn active_key(root: &Path, job_id: &str) -> String {
    format!("{}:{job_id}", digest(root.as_os_str().as_encoded_bytes()))
}

fn job_handle(
    job_id: String,
    source_handle: String,
    scope_revision: String,
) -> Result<Value, JobError> {
    serde_json::to_value(ModuleJobHandle { schema: SCHEMA, job_id, source_handle, scope_revision })
        .map_err(json_error)
}

fn module_live(module_id: &str, module_digest: &str) -> bool {
    Registry::for_user()
        .and_then(|registry| registry.load(module_id, true))
        .is_ok_and(|definition| definition.digest == module_digest)
}

fn validate_target_path(path: &str) -> Result<(), JobError> {
    if path.is_empty()
        || path.contains('\0')
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || Path::new(path).is_absolute()
        || !Path::new(path).components().all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(rejected(
            "invalid_target_path",
            "target path must be a canonical source-relative regular file",
        ));
    }
    Ok(())
}

fn prepare_copy_patch(
    source: &scoped_source::MaterializedSourceFile,
    destination: &Path,
    source_path: &str,
    patch: &CopyPatchSpec,
) -> Result<(scoped_source::MaterializedSourceFile, ModulePatchReceipt), JobError> {
    if patch.expected_source_digest != source.digest {
        return Err(rejected(
            "patch_preimage_mismatch",
            "the proposed patch does not name the immutable target preimage digest",
        ));
    }
    if patch.replacement.len() > PATCH_SOURCE_LENGTH
        || patch.case_id.is_empty()
        || patch.case_id.len() > 128
        || !patch
            .case_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || patch.expected_stdout_fragment.is_empty()
        || patch.expected_stdout_fragment.len() > 4096
        || patch.expected_stdout_fragment.contains('\0')
        || patch.findings.is_empty()
        || patch.findings.len() > 128
        || patch.prerequisites.len() > 128
        || patch.dependencies.len() > 128
        || patch
            .findings
            .iter()
            .chain(&patch.prerequisites)
            .chain(&patch.dependencies)
            .any(|value| value.is_empty() || value.len() > 4096 || value.contains('\0'))
    {
        return Err(rejected(
            "invalid_copy_patch",
            "copy patches require bounded replacement bytes, a stable case, expected output, findings, prerequisites and dependencies",
        ));
    }
    if fs::metadata(&source.path).map_err(io_error)?.len() > PATCH_SOURCE_BYTES {
        return Err(rejected(
            "patch_source_too_large",
            "phase-six copy patches currently require a source file no larger than 1 MiB",
        ));
    }
    let original = fs::read(&source.path).map_err(io_error)?;
    if digest(&original) != source.digest {
        return Err(failed(
            "patch_preimage_integrity",
            "materialized patch preimage no longer matches its immutable digest",
            false,
        ));
    }
    let replacement = patch.replacement.as_bytes();
    let patched_digest = digest(replacement);
    let diff_bytes = unified_diff(source_path, &original, replacement)?;
    let diff_artifact = digest(&diff_bytes);
    let patch_identity = serde_json::to_vec(&(
        "peritus-copy-patch-v1",
        &source.snapshot_digest,
        source_path,
        &source.digest,
        &patched_digest,
        &patch.findings,
        &patch.prerequisites,
        patch.order,
        &patch.dependencies,
    ))
    .map_err(json_error)?;
    let patch_digest = digest(&patch_identity);
    write_private(destination, replacement)?;
    let receipt = ModulePatchReceipt {
        patch_digest,
        source_path: source_path.into(),
        original_digest: source.digest.clone(),
        patched_digest: patched_digest.clone(),
        diff_artifact,
        diff_bytes,
        case_id: patch.case_id.clone(),
        findings: patch.findings.clone(),
        prerequisites: patch.prerequisites.clone(),
        order: patch.order,
        dependencies: patch.dependencies.clone(),
    };
    Ok((
        scoped_source::MaterializedSourceFile {
            snapshot_digest: source.snapshot_digest.clone(),
            path: destination.to_path_buf(),
            digest: patched_digest,
        },
        receipt,
    ))
}

fn unified_diff(path: &str, original: &[u8], replacement: &[u8]) -> Result<Vec<u8>, JobError> {
    let original = std::str::from_utf8(original).map_err(|_| {
        rejected("binary_patch_unsupported", "phase-six copy patches require UTF-8 source text")
    })?;
    let replacement = std::str::from_utf8(replacement).map_err(|_| {
        rejected(
            "binary_patch_unsupported",
            "phase-six copy patches require UTF-8 replacement text",
        )
    })?;
    let original_lines = line_count(original);
    let replacement_lines = line_count(replacement);
    let original_range = hunk_range(original_lines);
    let replacement_range = hunk_range(replacement_lines);
    let mut diff = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{original_range} +{replacement_range} @@\n"
    );
    append_diff_lines(&mut diff, '-', original);
    append_diff_lines(&mut diff, '+', replacement);
    Ok(diff.into_bytes())
}

fn line_count(text: &str) -> usize {
    if text.is_empty() { 0 } else { text.split_inclusive('\n').count() }
}

fn hunk_range(lines: usize) -> String {
    if lines == 0 { "0,0".into() } else { format!("1,{lines}") }
}

fn append_diff_lines(diff: &mut String, prefix: char, text: &str) {
    for line in text.split_inclusive('\n') {
        diff.push(prefix);
        if let Some(content) = line.strip_suffix('\n') {
            diff.push_str(content);
            diff.push('\n');
        } else {
            diff.push_str(line);
            diff.push('\n');
            diff.push_str("\\ No newline at end of file\n");
        }
    }
}

fn random_canary() -> Result<Vec<u8>, JobError> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(|error| failed("job_random", &error.to_string(), true))?;
    Ok(hex(&random).into_bytes())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), JobError> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o400);
    }
    let mut file = options.open(path).map_err(io_error)?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(io_error)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

fn page(bytes: &[u8], offset: u64, maximum: usize) -> Result<(u64, Vec<u8>), JobError> {
    let total = u64::try_from(bytes.len()).map_err(|_| representation())?;
    if offset > total {
        return Err(rejected("job_log_range", "log offset exceeds retained bytes"));
    }
    let start = usize::try_from(offset).map_err(|_| representation())?;
    let end = start.saturating_add(maximum).min(bytes.len());
    Ok((total, bytes[start..end].to_vec()))
}

fn load_index_locked(root: &Path) -> Result<JobIndex, JobError> {
    let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
    load_index(root)
}

fn load_index(root: &Path) -> Result<JobIndex, JobError> {
    match fs::read(root.join("index.json")) {
        Ok(bytes) => {
            let index: JobIndex = serde_json::from_slice(&bytes).map_err(|_| {
                failed("job_state_integrity", "job index is not valid schema-one JSON", false)
            })?;
            if index.schema != SCHEMA {
                return Err(failed(
                    "job_state_integrity",
                    "job index has an unsupported schema",
                    false,
                ));
            }
            Ok(index)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(JobIndex::default()),
        Err(error) => Err(io_error(error)),
    }
}

fn save_index(root: &Path, index: &JobIndex) -> Result<(), JobError> {
    fs::create_dir_all(root).map_err(io_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(root).map_err(io_error)?;
    serde_json::to_writer(&mut temporary, index).map_err(json_error)?;
    temporary.write_all(b"\n").map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary.persist(root.join("index.json")).map_err(|error| io_error(error.error))?;
    #[cfg(unix)]
    fs::File::open(root).and_then(|directory| directory.sync_all()).map_err(io_error)?;
    Ok(())
}

fn publish_bytes(path: &Path, bytes: &[u8]) -> Result<(), JobError> {
    let parent = path.parent().ok_or_else(representation)?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    temporary.write_all(bytes).map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary.persist(path).map_err(|error| io_error(error.error))?;
    Ok(())
}

fn publish_dependency(store: &ArtifactStore, bytes: &[u8]) -> Result<String, String> {
    let artifact = ArtifactDigest::new(Sha256::digest(bytes).into());
    let size = u64::try_from(bytes.len()).map_err(|error| error.to_string())?;
    let request = WriteRequest::new(
        artifact,
        size,
        size.max(1),
        MediaType::new("application/vnd.peritus.prepared-dependency.v1")
            .map_err(|error| error.to_string())?,
        EncryptionMetadata::unencrypted(),
        event_id(&artifact.to_hex()),
    );
    let mut writer = store.begin_write(request).map_err(|error| error.to_string())?;
    writer.write_chunk(bytes).map_err(|error| error.to_string())?;
    writer.finalize().map_err(|error| error.to_string())?;
    Ok(artifact.to_hex())
}

fn sandbox_path(path: &Path) -> Result<SandboxPath, JobError> {
    SandboxPath::new(path.to_str().ok_or_else(|| rejected("job_path", "job path is not UTF-8"))?)
        .map_err(sandbox_error)
}

fn event_id(value: &str) -> EventId {
    let digest = Sha256::digest(value.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[0] |= 1;
    EventId::new(bytes).expect("nonzero job event identity")
}

fn valid_job_id(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, JobError> {
    serde_json::from_value(value).map_err(|_| {
        rejected("invalid_job_input", "owned-job input does not match contract version 1")
    })
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug)]
enum JobError {
    Rejected { code: String, detail: String },
    Failed { code: String, detail: String, retryable: bool },
}

fn rejected(code: &str, detail: &str) -> JobError {
    JobError::Rejected { code: code.into(), detail: detail.into() }
}

fn failed(code: &str, detail: &str, retryable: bool) -> JobError {
    JobError::Failed { code: code.into(), detail: detail.into(), retryable }
}

fn source_error(error: scoped_source::BrokerError) -> JobError {
    match error {
        scoped_source::BrokerError::Rejected { code, detail } => {
            JobError::Rejected { code, detail }
        }
        scoped_source::BrokerError::Failed { code, detail, retryable } => {
            JobError::Failed { code, detail, retryable }
        }
    }
}

fn state_poisoned() -> JobError {
    failed("job_state", "job ownership state is poisoned", false)
}

fn representation() -> JobError {
    failed("job_representation", "job value exceeds durable representation", false)
}

fn io_error(error: std::io::Error) -> JobError {
    failed("job_io", &error.to_string(), true)
}

fn json_error(error: serde_json::Error) -> JobError {
    failed("job_json", &error.to_string(), false)
}

fn runtime_error(error: crate::ProductRunnerError) -> JobError {
    failed("job_authority", &error.to_string(), false)
}

fn process_error(error: peritus_process::ProcessError) -> JobError {
    failed("job_process", &error.to_string(), false)
}

fn sandbox_error(error: peritus_sandbox::SandboxError) -> JobError {
    failed("job_sandbox", &error.to_string(), false)
}

fn linux_error(error: peritus_sandbox_linux::LinuxError) -> JobError {
    failed("job_linux_backend", &error.to_string(), false)
}
