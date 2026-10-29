//! Generic disposable source-copy execution for native modules.
//!
//! This host capability knows nothing about security classes, findings, campaigns, adapters, or
//! repair policy. It copies an explicitly scoped immutable tree, executes caller-selected literal
//! argv through the ordinary Peritus sandbox, and returns raw lifecycle and process observations.

use super::{ProductRunInput, scoped_source};
use crate::control::PermissionCapability;
use flate2::read::GzDecoder;
use peritus_modules::{Registry, preparation};
use peritus_plugin_sdk::{
    MODULE_WORKER_FEATURE_BROWSER_AUTOMATION, MODULE_WORKER_FEATURE_BUN_RUNTIME,
    MODULE_WORKER_FEATURE_CARGO_DEPENDENCIES, ModuleHostOutcome, ModuleHostPlatform,
    ModuleLaboratoryCatalog, ModuleLaboratoryHandle, ModuleLaboratoryInvokeRequest,
    ModuleLaboratoryLifecycleRequest, ModuleLaboratoryPhase, ModuleLaboratoryProcessInput,
    ModuleLaboratoryReceipt, ModuleLaboratoryRecipe, ModuleLaboratoryResult,
    ModuleLaboratorySourceCreateRequest, ModuleLaboratoryStatus, ModuleLaboratoryTelemetry,
    ModuleWorkerRequest, ModuleWorkerResult, ScopedSourceAuthority, ScopedSourceVerification,
    ScopedSourceVerifyRequest,
};
use peritus_process::{
    CancellationReason, CommandSpec, DeadlinePolicy, EnvironmentPlan, EnvironmentVariable,
    ExecutionPlan, GracefulAction, IoMode, NativeSandboxBackend, OsExitObservation,
    OutputOverflowAction, OutputPolicy, OutputStream, ProcessControl, ProcessResourcePolicy,
    StdinPolicy, TerminalResult, WorkingDirectory, WorkspaceAccess,
};
use peritus_sandbox::{
    AdmissionProfile, DescendantPolicy, EnvironmentContract, EnvironmentMode, EnvironmentName,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, InputPermission, IsolationRequirement, NetworkContract, PathScope,
    ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TreeContainment, admit_backend,
    compile_sandbox,
};
use peritus_sandbox_linux::{LinuxBackend, LinuxBackendConfig};
use peritus_types::ResourceQuantity;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const SCHEMA: u16 = 1;
const SOURCE_RECIPE: &str = "linux.complete-source.v1";
const PROCESS_OPERATION: &str = "process.execute.v1";
const OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const CARGO_DEPENDENCY_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const CARGO_DEPENDENCY_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const CARGO_DEPENDENCY_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const CARGO_DEPENDENCY_FILES: u64 = 200_000;
const CARGO_DEPENDENCY_PACKAGES: usize = 4_096;
const SOURCE_PROCESS_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const CARGO_PROCESS_MEMORY_CAP_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const CARGO_PROCESS_MEMORY_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema: u16,
    laboratories: BTreeMap<String, StoredLaboratory>,
}

impl Default for Index {
    fn default() -> Self {
        Self { schema: SCHEMA, laboratories: BTreeMap::new() }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredLaboratory {
    module_id: String,
    module_digest: String,
    source: ScopedSourceAuthority,
    source_digest: String,
    secret_digest: String,
    source_tree_files: u64,
    source_tree_bytes: u64,
    status: ModuleLaboratoryStatus,
    receipts: Vec<ModuleLaboratoryReceipt>,
}

#[derive(Debug)]
enum LaboratoryError {
    Rejected { code: String, detail: String },
    Failed { code: String, detail: String, retryable: bool },
}

struct SourceInvocationMaterial {
    source_root: PathBuf,
    source_digest: String,
    secret: Vec<u8>,
    authority: ScopedSourceAuthority,
}

struct SourceProcessObservation {
    terminal: TerminalResult,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    execution_plan_digest: String,
}

struct SourceProcessResult {
    observation: Value,
    oracle_matched: bool,
    cleanup: bool,
    output_bytes: u64,
}

#[derive(Default)]
struct WorkerRuntimeFeatures {
    environment: BTreeMap<String, String>,
    read_only_roots: Vec<PathBuf>,
    read_private_proc: bool,
    read_random_device: bool,
    prepare_cargo_dependencies: bool,
}

#[derive(Clone, Copy, Default)]
struct SourceProcessFeatures<'a> {
    roots: &'a [PathBuf],
    private_proc: bool,
    random_device: bool,
}

fn state_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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
            root: host_state.join("source-laboratories-v1"),
            source_state,
            module_id,
            module_digest,
        }
    }

    pub(super) fn dispatch(&self, key: &str, operation: &str, input: Value) -> ModuleHostOutcome {
        let result = match operation {
            "catalog" => Self::catalog(),
            "create-source" => decode(input).and_then(|request| self.create_source(key, request)),
            "prepare" => decode(input).and_then(|request| self.prepare(&request)),
            "launch" => decode(input).and_then(|request| self.launch(&request)),
            "status" => decode(input).and_then(|request| self.status(&request)),
            "invoke" => decode(input).and_then(|request| self.invoke(&request)),
            "snapshot" => decode(input).and_then(|request| self.snapshot(&request)),
            "reset" => decode(input).and_then(|request| self.reset(&request)),
            "stop" => decode(input).and_then(|request| self.stop(&request)),
            "teardown" => decode(input).and_then(|request| self.teardown(&request)),
            _ => {
                return ModuleHostOutcome::Unsupported {
                    detail: format!(
                        "application-laboratory operation {operation} is unavailable in contract version 1"
                    ),
                };
            }
        };
        match result {
            Ok(output) => ModuleHostOutcome::Success { output },
            Err(LaboratoryError::Rejected { code, detail }) => {
                ModuleHostOutcome::Rejected { code, detail }
            }
            Err(LaboratoryError::Failed { code, detail, retryable }) => {
                ModuleHostOutcome::Failed { code, detail, retryable_with_new_action: retryable }
            }
        }
    }

    /// Executes an opaque module-defined worker in a private copy of explicitly scoped source.
    ///
    /// The host validates only generic authority, identity, resource, lifecycle, and byte bounds.
    /// It deliberately does not interpret the worker name, request body, or returned bytes.
    pub(super) fn execute_worker(
        &self,
        module_executable: &Path,
        input: Value,
    ) -> ModuleHostOutcome {
        let result = decode::<ModuleWorkerRequest>(input)
            .and_then(|request| self.run_worker(module_executable, &request));
        match result {
            Ok(output) => ModuleHostOutcome::Success { output },
            Err(LaboratoryError::Rejected { code, detail }) => {
                ModuleHostOutcome::Rejected { code, detail }
            }
            Err(LaboratoryError::Failed { code, detail, retryable }) => {
                ModuleHostOutcome::Failed { code, detail, retryable_with_new_action: retryable }
            }
        }
    }

    fn run_worker(
        &self,
        module_executable: &Path,
        request: &ModuleWorkerRequest,
    ) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_worker_request(request)?;
        let mut runtime = worker_runtime_features(&request.runtime_features)?;
        let broker = scoped_source::Broker::new(self.input, self.source_state, self.module_digest);
        let snapshot = broker.validate_authority(&request.source).map_err(source_error)?;
        // Worker source copies may be much larger than the host's shared temporary-filesystem
        // quota. Keep them in the user's private cache, using a deliberately short directory name
        // so runtimes with fixed-size AF_UNIX socket paths retain essentially the same headroom as
        // they had under `/tmp`.
        let private = worker_private_directory()?;
        let source_root = private.path().join("source");
        let tree = broker
            .materialize_tree(
                &request.source,
                &source_root,
                100_000,
                request.limits.storage_bytes.min(2 * 1024 * 1024 * 1024),
            )
            .map_err(source_error)?;
        if tree.snapshot_digest != snapshot {
            return Err(failed(
                "worker_source",
                "materialized worker source did not retain its immutable snapshot identity",
                false,
            ));
        }
        for protected in [".git", ".peritus", ".crosslink"] {
            fs::create_dir_all(source_root.join(protected)).map_err(io_error)?;
        }
        if runtime.prepare_cargo_dependencies {
            prepare_cargo_dependencies(&source_root, &self.input.cancelled)?;
            let cargo_home = source_root.join(".peritus-cargo-home");
            fs::create_dir_all(&cargo_home).map_err(io_error)?;
            runtime.environment.insert("CARGO_HOME".into(), cargo_home.to_string_lossy().into());
        }
        let executable = fs::canonicalize(module_executable).map_err(io_error)?;
        let process_input = worker_process_input(request, runtime.environment)?;
        let command =
            ResolvedSourceCommand { executable, arguments: process_input.arguments.clone() };
        let context = self
            .input
            .command_runtime
            .reserve_native_module_job_context_with_environment(
                self.input.run_id,
                source_environment_names(&process_input),
            )
            .map_err(|error| failed("worker_runtime", &error.to_string(), true))?;
        let config = self.input.module_jobs.as_ref().ok_or_else(|| {
            rejected(
                "worker_backend",
                "this Peritus host has no configured native Linux isolation backend",
            )
        })?;
        let prepared = prepare_source_process(
            config,
            &context.identity(),
            &source_root,
            &command,
            &process_input,
            worker_resources(request)?,
            SourceProcessFeatures {
                roots: &runtime.read_only_roots,
                private_proc: runtime.read_private_proc,
                random_device: runtime.read_random_device,
            },
        )?;
        let observed =
            self.execute_source_process(context, prepared, &process_input, &request.source)?;
        let verification = match broker.dispatch(
            "verify",
            serde_json::to_value(ScopedSourceVerifyRequest {
                schema: SCHEMA,
                authority: request.source.clone(),
            })
            .map_err(json_error)?,
        ) {
            ModuleHostOutcome::Success { output } => {
                serde_json::from_value::<ScopedSourceVerification>(output).map_err(json_error)?
            }
            ModuleHostOutcome::Rejected { code, detail }
            | ModuleHostOutcome::Failed { code, detail, .. } => {
                return Err(failed(&code, &detail, false));
            }
            ModuleHostOutcome::Unsupported { detail } => {
                return Err(failed("worker_source_verification", &detail, false));
            }
        };
        let exit_code = match observed.terminal.os_exit() {
            OsExitObservation::Code(code) => Some(*code),
            _ => None,
        };
        encode(ModuleWorkerResult {
            schema: SCHEMA,
            worker: request.worker.clone(),
            terminal_disposition: format!("{:?}", observed.terminal.disposition())
                .to_ascii_lowercase(),
            exit_code,
            stdout: observed.stdout,
            stderr: observed.stderr,
            execution_plan_digest: observed.execution_plan_digest,
            cleanup_complete: terminal_cleanup_complete(&observed.terminal),
            original_source_unchanged: verification.unchanged,
        })
    }

    fn catalog() -> Result<Value, LaboratoryError> {
        encode(ModuleLaboratoryCatalog { schema: SCHEMA, recipes: vec![source_recipe()] })
    }

    fn create_source(
        &self,
        key: &str,
        request: ModuleLaboratorySourceCreateRequest,
    ) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_schema(request.schema)?;
        let snapshot =
            scoped_source::Broker::new(self.input, self.source_state, self.module_digest)
                .validate_authority(&request.source)
                .map_err(source_error)?;
        let laboratory_id = digest(
            format!(
                "peritus.source-laboratory.v1\0{}\0{}\0{}\0{key}",
                self.module_id, self.module_digest, snapshot,
            )
            .as_bytes(),
        );
        let status = ModuleLaboratoryStatus {
            schema: SCHEMA,
            handle: handles(
                &laboratory_id,
                &request.source.scope_revision,
                &snapshot,
                SOURCE_RECIPE,
            ),
            phase: ModuleLaboratoryPhase::Created,
            telemetry: empty_telemetry(),
            snapshots: 0,
            receipts: 0,
            detail: "Laboratory identity is bound to the complete immutable source snapshot."
                .into(),
            retained_error: None,
            cleanup_complete: false,
        };
        let stored = StoredLaboratory {
            module_id: self.module_id.into(),
            module_digest: self.module_digest.into(),
            source: request.source,
            source_digest: snapshot,
            secret_digest: String::new(),
            source_tree_files: 0,
            source_tree_bytes: 0,
            status,
            receipts: Vec::new(),
        };
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let mut index = load(&self.root)?;
        let status = if let Some(existing) = index.laboratories.get(&laboratory_id) {
            require_owner(existing, self.module_id, self.module_digest)?;
            existing.status.clone()
        } else {
            let status = stored.status.clone();
            index.laboratories.insert(laboratory_id, stored);
            save(&self.root, &index)?;
            status
        };
        result(status, None)
    }

    fn prepare(
        &self,
        request: &ModuleLaboratoryLifecycleRequest,
    ) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_lifecycle(request)?;
        let directory = laboratory_directory(&self.root, &request.laboratory_id);
        let private = directory.join("private");
        let mut prepared = {
            let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
            let index = load(&self.root)?;
            let lab = owned(&index, &request.laboratory_id, self.module_id, self.module_digest)?;
            if lab.status.phase != ModuleLaboratoryPhase::Created {
                return Err(rejected(
                    "laboratory_phase",
                    "only a newly created laboratory can be prepared",
                ));
            }
            if let Ok(metadata) = fs::symlink_metadata(&directory) {
                if metadata.file_type().is_symlink() {
                    return Err(failed(
                        "laboratory_orphan",
                        "laboratory storage is an unexpected symbolic link",
                        false,
                    ));
                }
                fs::remove_dir_all(&directory).map_err(|error| {
                    failed(
                        "laboratory_orphan",
                        &format!(
                            "interrupted disposable laboratory storage could not be replaced: {error}"
                        ),
                        true,
                    )
                })?;
            }
            fs::create_dir_all(directory.parent().ok_or_else(|| {
                failed("laboratory_path", "laboratory storage has no parent", false)
            })?)
            .map_err(io_error)?;
            fs::create_dir(&directory).map_err(io_error)?;
            fs::create_dir(&private).map_err(io_error)?;
            lab.clone()
        };
        let broker = scoped_source::Broker::new(self.input, self.source_state, self.module_digest);
        let receipt = match prepare_complete_source(
            &mut prepared,
            request,
            &private,
            &broker,
            &self.input.cancelled,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = fs::remove_dir_all(&directory);
                return Err(error);
            }
        };
        let status = prepared.status.clone();
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        let lab =
            owned_mut(&mut index, &request.laboratory_id, self.module_id, self.module_digest)?;
        if lab.status.phase != ModuleLaboratoryPhase::Created {
            let _ = fs::remove_dir_all(&directory);
            return Err(failed(
                "laboratory_phase",
                "laboratory state changed while source preparation was in progress",
                false,
            ));
        }
        *lab = prepared;
        save(&self.root, &index)?;
        result(status, Some(receipt))
    }

    fn launch(&self, request: &ModuleLaboratoryLifecycleRequest) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_lifecycle(request)?;
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        let lab =
            owned_mut(&mut index, &request.laboratory_id, self.module_id, self.module_digest)?;
        if !matches!(
            lab.status.phase,
            ModuleLaboratoryPhase::Prepared | ModuleLaboratoryPhase::Stopped
        ) {
            return Err(rejected(
                "laboratory_phase",
                "only a prepared or stopped laboratory can be opened",
            ));
        }
        let source_root =
            laboratory_directory(&self.root, &request.laboratory_id).join("private/source");
        if !source_root.is_dir() {
            return Err(failed(
                "laboratory_source",
                "the complete disposable source copy is unavailable",
                false,
            ));
        }
        lab.status.phase = ModuleLaboratoryPhase::Ready;
        lab.status.detail =
            "Disposable source copy is ready for bounded process.execute.v1 requests.".into();
        let receipt = receipt(
            lab,
            "launch",
            digest(request.laboratory_id.as_bytes()),
            json!({
                "interface": PROCESS_OPERATION,
                "persistent_target_process": false,
                "source_files": lab.source_tree_files,
                "source_bytes": lab.source_tree_bytes,
            }),
            true,
            None,
        );
        lab.receipts.push(receipt.clone());
        lab.status.receipts = count(&lab.receipts)?;
        let status = lab.status.clone();
        save(&self.root, &index)?;
        result(status, Some(receipt))
    }

    fn status(&self, request: &ModuleLaboratoryLifecycleRequest) -> Result<Value, LaboratoryError> {
        self.require_read_authority()?;
        validate_lifecycle(request)?;
        self.current_result(&request.laboratory_id, false)
    }

    fn invoke(&self, request: &ModuleLaboratoryInvokeRequest) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_schema(request.schema)?;
        if !valid_id(&request.laboratory_id) {
            return Err(rejected("laboratory_identity", "invalid laboratory identity"));
        }
        if request.operation != PROCESS_OPERATION {
            return Err(rejected(
                "laboratory_operation",
                "source laboratories accept only process.execute.v1",
            ));
        }
        let input: ModuleLaboratoryProcessInput = decode(request.input.clone())?;
        self.invoke_source_process(request, &input)
    }

    fn invoke_source_process(
        &self,
        request: &ModuleLaboratoryInvokeRequest,
        input: &ModuleLaboratoryProcessInput,
    ) -> Result<Value, LaboratoryError> {
        validate_process_input(input)?;
        let config = self.input.module_jobs.as_ref().ok_or_else(|| {
            rejected(
                "laboratory_backend",
                "this Peritus host has no configured native Linux isolation backend",
            )
        })?;
        let material = self.source_invocation_material(request)?;
        let resolved = resolve_source_command(&material.source_root, input)?;
        let context = self
            .input
            .command_runtime
            .reserve_native_module_job_context_with_environment(
                self.input.run_id,
                source_environment_names(input),
            )
            .map_err(|error| failed("laboratory_runtime", &error.to_string(), true))?;
        let resources = source_resources(input)?;
        let prepared = prepare_source_process(
            config,
            &context.identity(),
            &material.source_root,
            &resolved,
            input,
            resources,
            SourceProcessFeatures::default(),
        )?;
        let observed =
            self.execute_source_process(context, prepared, input, &material.authority)?;
        if contains_secret(&observed.stdout, &material.secret)
            || contains_secret(&observed.stderr, &material.secret)
        {
            return self.quarantine(
                &request.laboratory_id,
                "target output contained host-private laboratory canary material",
            );
        }
        let request_bytes = serde_json::to_vec(input).map_err(json_error)?;
        let process_result = source_process_result(input, resources, &observed);
        self.record_source_process(request, &material, &request_bytes, process_result)
    }

    fn source_invocation_material(
        &self,
        request: &ModuleLaboratoryInvokeRequest,
    ) -> Result<SourceInvocationMaterial, LaboratoryError> {
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let index = load(&self.root)?;
        let lab = owned(&index, &request.laboratory_id, self.module_id, self.module_digest)?;
        if lab.status.phase != ModuleLaboratoryPhase::Ready {
            return Err(rejected("laboratory_phase", "source laboratory is not ready"));
        }
        if request.endpoint_handle != lab.status.handle.endpoint_handle {
            return Err(rejected(
                "laboratory_endpoint",
                "the opaque endpoint does not belong to this laboratory",
            ));
        }
        let private = laboratory_directory(&self.root, &request.laboratory_id).join("private");
        Ok(SourceInvocationMaterial {
            source_root: private.join("source"),
            source_digest: lab.source_digest.clone(),
            secret: fs::read(private.join("secret")).map_err(io_error)?,
            authority: lab.source.clone(),
        })
    }

    fn execute_source_process(
        &self,
        context: crate::RestrictedProcessContext,
        prepared: PreparedProcess,
        input: &ModuleLaboratoryProcessInput,
        authority: &ScopedSourceAuthority,
    ) -> Result<SourceProcessObservation, LaboratoryError> {
        let execution_plan_digest = hex(prepared.plan.digest().as_bytes());
        let process = self
            .input
            .command_runtime
            .launch_native_module_job(
                context,
                prepared.plan,
                &prepared.sandbox,
                &prepared.admission,
                prepared.backend,
            )
            .map_err(|error| failed("laboratory_launch", &error.to_string(), true))?;
        let control = process.control();
        if !input.stdin.is_empty() {
            control.write_stdin(input.stdin.clone()).map_err(process_error)?;
            control.close_stdin().map_err(process_error)?;
        }
        let (done, monitor) = self.spawn_source_monitor(control.clone(), authority.clone());
        let terminal = process.wait();
        done.store(true, Ordering::Release);
        let _ = monitor.join();
        let terminal = terminal.map_err(process_error)?;
        let stdout = read_process_output(&control, OutputStream::Stdout)?;
        let stderr = read_process_output(&control, OutputStream::Stderr)?;
        Ok(SourceProcessObservation { terminal, stdout, stderr, execution_plan_digest })
    }

    fn spawn_source_monitor(
        &self,
        control: ProcessControl,
        authority: ScopedSourceAuthority,
    ) -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
        let cancelled = Arc::clone(&self.input.cancelled);
        let provider_cancellation = self.input.provider_cancellation.clone();
        let conversation = Arc::clone(&self.input.conversation);
        let source_state = self.source_state.to_path_buf();
        let module_id = self.module_id.to_owned();
        let module_digest = self.module_digest.to_owned();
        let done = Arc::new(AtomicBool::new(false));
        let monitor_done = Arc::clone(&done);
        let monitor = thread::spawn(move || {
            while !monitor_done.load(Ordering::Acquire) {
                let permissions = conversation.effective_permissions();
                let permissions_live = permissions.allows(PermissionCapability::Read)
                    && permissions.allows(PermissionCapability::Write)
                    && permissions.allows(PermissionCapability::Process);
                if cancelled.load(Ordering::Acquire)
                    || provider_cancellation.is_cancelled()
                    || !permissions_live
                    || !scoped_source::authority_live(&source_state, &authority)
                    || !module_live(&module_id, &module_digest)
                {
                    let _ = control.cancel(CancellationReason::User);
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        (done, monitor)
    }

    fn record_source_process(
        &self,
        request: &ModuleLaboratoryInvokeRequest,
        material: &SourceInvocationMaterial,
        request_bytes: &[u8],
        process_result: SourceProcessResult,
    ) -> Result<Value, LaboratoryError> {
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        let lab =
            owned_mut(&mut index, &request.laboratory_id, self.module_id, self.module_digest)?;
        if lab.source_digest != material.source_digest {
            return Err(failed(
                "laboratory_identity",
                "source digest changed during process execution",
                false,
            ));
        }
        if lab.source != material.authority {
            return Err(failed(
                "laboratory_identity",
                "source authority changed during process execution",
                false,
            ));
        }
        lab.status.telemetry.requests = lab.status.telemetry.requests.saturating_add(1);
        lab.status.telemetry.input_bytes = lab
            .status
            .telemetry
            .input_bytes
            .saturating_add(u64::try_from(request_bytes.len()).unwrap_or(u64::MAX));
        lab.status.telemetry.output_bytes =
            lab.status.telemetry.output_bytes.saturating_add(process_result.output_bytes);
        lab.status.telemetry.host_loopback_blocked = true;
        lab.status.telemetry.credential_output_blocked = true;
        lab.status.detail = if process_result.oracle_matched {
            "Source process completed and satisfied the caller-declared result predicate."
        } else {
            "Source process completed; the caller-declared result predicate did not match."
        }
        .into();
        let receipt = receipt(
            lab,
            PROCESS_OPERATION,
            digest(request_bytes),
            process_result.observation,
            process_result.cleanup,
            None,
        );
        lab.receipts.push(receipt.clone());
        lab.status.receipts = count(&lab.receipts)?;
        let status = lab.status.clone();
        save(&self.root, &index)?;
        result(status, Some(receipt))
    }

    fn snapshot(
        &self,
        request: &ModuleLaboratoryLifecycleRequest,
    ) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_lifecycle(request)?;
        let source_root =
            laboratory_directory(&self.root, &request.laboratory_id).join("private/source");
        let (state_digest, files, bytes) = source_tree_digest(&source_root)?;
        let snapshots = laboratory_directory(&self.root, &request.laboratory_id).join("snapshots");
        fs::create_dir_all(&snapshots).map_err(io_error)?;
        let marker = snapshots.join(format!("{state_digest}.source-tree"));
        if !marker.exists() {
            write_atomic(&marker, format!("{files}\n{bytes}\n").as_bytes())?;
        }
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        let lab =
            owned_mut(&mut index, &request.laboratory_id, self.module_id, self.module_digest)?;
        if lab.status.phase != ModuleLaboratoryPhase::Ready {
            return Err(rejected("laboratory_phase", "only a ready laboratory can be snapshotted"));
        }
        lab.status.telemetry.state_digest.clone_from(&state_digest);
        lab.status.snapshots = fs::read_dir(&snapshots)
            .map_err(io_error)?
            .filter_map(Result::ok)
            .count()
            .try_into()
            .map_err(|_| failed("laboratory_state", "snapshot count overflow", false))?;
        let receipt = receipt(
            lab,
            "snapshot",
            digest(request.laboratory_id.as_bytes()),
            json!({"source_tree_digest": state_digest, "files": files, "bytes": bytes}),
            true,
            None,
        );
        lab.receipts.push(receipt.clone());
        lab.status.receipts = count(&lab.receipts)?;
        let status = lab.status.clone();
        save(&self.root, &index)?;
        result(status, Some(receipt))
    }

    fn reset(&self, request: &ModuleLaboratoryLifecycleRequest) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_lifecycle(request)?;
        let (authority, snapshot) = {
            let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
            let index = load(&self.root)?;
            let lab = owned(&index, &request.laboratory_id, self.module_id, self.module_digest)?;
            if lab.status.phase != ModuleLaboratoryPhase::Ready {
                return Err(rejected("laboratory_phase", "only a ready laboratory can be reset"));
            }
            (lab.source.clone(), lab.status.handle.source_snapshot.clone())
        };
        let source_root =
            laboratory_directory(&self.root, &request.laboratory_id).join("private/source");
        if source_root.exists() {
            fs::remove_dir_all(&source_root).map_err(io_error)?;
        }
        let tree = scoped_source::Broker::new(self.input, self.source_state, self.module_digest)
            .materialize_tree(&authority, &source_root, 100_000, 2 * 1024 * 1024 * 1024)
            .map_err(source_error)?;
        if tree.snapshot_digest != snapshot {
            return Err(failed(
                "laboratory_source",
                "reset source tree did not retain the immutable snapshot identity",
                false,
            ));
        }
        for protected in [".git", ".peritus", ".crosslink"] {
            fs::create_dir_all(source_root.join(protected)).map_err(io_error)?;
        }
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        let lab =
            owned_mut(&mut index, &request.laboratory_id, self.module_id, self.module_digest)?;
        lab.source_tree_files = tree.file_count;
        lab.source_tree_bytes = tree.total_bytes;
        lab.status.telemetry.state_digest =
            digest(format!("{}\0{}", tree.file_count, tree.total_bytes).as_bytes());
        let receipt = receipt(
            lab,
            "reset",
            digest(request.laboratory_id.as_bytes()),
            json!({"immutable_source_restored": true, "files": tree.file_count, "bytes": tree.total_bytes}),
            true,
            None,
        );
        lab.receipts.push(receipt.clone());
        lab.status.receipts = count(&lab.receipts)?;
        let status = lab.status.clone();
        save(&self.root, &index)?;
        result(status, Some(receipt))
    }

    fn stop(&self, request: &ModuleLaboratoryLifecycleRequest) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_lifecycle(request)?;
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        let lab =
            owned_mut(&mut index, &request.laboratory_id, self.module_id, self.module_digest)?;
        if !matches!(
            lab.status.phase,
            ModuleLaboratoryPhase::Ready
                | ModuleLaboratoryPhase::Prepared
                | ModuleLaboratoryPhase::Stopped
                | ModuleLaboratoryPhase::Failed
        ) {
            return Err(rejected("laboratory_phase", "laboratory is not stoppable"));
        }
        lab.status.phase = ModuleLaboratoryPhase::Stopped;
        lab.status.detail = "No source process remains owned between invocations.".into();
        let receipt = receipt(
            lab,
            "stop",
            digest(request.laboratory_id.as_bytes()),
            json!({"owned_process_tree_stopped": true}),
            true,
            None,
        );
        lab.receipts.push(receipt.clone());
        lab.status.receipts = count(&lab.receipts)?;
        let status = lab.status.clone();
        save(&self.root, &index)?;
        result(status, Some(receipt))
    }

    fn teardown(
        &self,
        request: &ModuleLaboratoryLifecycleRequest,
    ) -> Result<Value, LaboratoryError> {
        self.require_effect_authority()?;
        validate_lifecycle(request)?;
        let source = {
            let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
            let index = load(&self.root)?;
            let lab = owned(&index, &request.laboratory_id, self.module_id, self.module_digest)?;
            if !matches!(
                lab.status.phase,
                ModuleLaboratoryPhase::Stopped
                    | ModuleLaboratoryPhase::Prepared
                    | ModuleLaboratoryPhase::Created
                    | ModuleLaboratoryPhase::Failed
                    | ModuleLaboratoryPhase::Quarantined
            ) {
                return Err(rejected(
                    "laboratory_phase",
                    "laboratory must be inactive before teardown",
                ));
            }
            lab.source.clone()
        };
        let (verification, verification_error) =
            match scoped_source::Broker::new(self.input, self.source_state, self.module_digest)
                .dispatch(
                    "verify",
                    serde_json::to_value(ScopedSourceVerifyRequest {
                        schema: SCHEMA,
                        authority: source,
                    })
                    .map_err(json_error)?,
                ) {
                ModuleHostOutcome::Success { output } => (
                    Some(
                        serde_json::from_value::<ScopedSourceVerification>(output)
                            .map_err(json_error)?,
                    ),
                    None,
                ),
                ModuleHostOutcome::Rejected { code, detail }
                | ModuleHostOutcome::Failed { code, detail, .. } => {
                    (None, Some(json!({"code": code, "detail": detail})))
                }
                ModuleHostOutcome::Unsupported { detail } => {
                    (None, Some(json!({"code": "laboratory_source", "detail": detail})))
                }
            };
        let directory = laboratory_directory(&self.root, &request.laboratory_id);
        if directory.exists() {
            fs::remove_dir_all(&directory).map_err(io_error)?;
        }
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        let lab =
            owned_mut(&mut index, &request.laboratory_id, self.module_id, self.module_digest)?;
        let unchanged = verification.as_ref().map(|value| value.unchanged);
        lab.status.phase = ModuleLaboratoryPhase::TornDown;
        lab.status.cleanup_complete = true;
        lab.status.detail = match unchanged {
            Some(true) => {
                "Disposable bytes were removed; the selected original source remains unchanged."
            }
            Some(false) => {
                "Disposable bytes were removed, but original source no longer matches the snapshot."
            }
            None => "Disposable bytes were removed; original-source verification was unavailable.",
        }
        .into();
        let receipt = receipt(
            lab,
            "teardown",
            digest(request.laboratory_id.as_bytes()),
            json!({
                "disposable_bytes_removed": true,
                "original_source_verification": verification,
                "original_source_verification_error": verification_error,
            }),
            true,
            unchanged,
        );
        lab.receipts.push(receipt.clone());
        lab.status.receipts = count(&lab.receipts)?;
        let status = lab.status.clone();
        save(&self.root, &index)?;
        result(status, Some(receipt))
    }

    fn current_result(
        &self,
        laboratory_id: &str,
        require_ready: bool,
    ) -> Result<Value, LaboratoryError> {
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let index = load(&self.root)?;
        let lab = owned(&index, laboratory_id, self.module_id, self.module_digest)?;
        if require_ready && lab.status.phase != ModuleLaboratoryPhase::Ready {
            return Err(rejected("laboratory_phase", "laboratory is not ready"));
        }
        result(lab.status.clone(), None)
    }

    fn quarantine<T>(&self, laboratory_id: &str, detail: &str) -> Result<T, LaboratoryError> {
        let _guard = state_lock().lock().map_err(|_| state_poisoned())?;
        let mut index = load(&self.root)?;
        if let Some(lab) = index.laboratories.get_mut(laboratory_id) {
            lab.status.phase = ModuleLaboratoryPhase::Quarantined;
            lab.status.retained_error = Some(detail.into());
            lab.status.detail = "Laboratory is quarantined pending explicit teardown.".into();
            save(&self.root, &index)?;
        }
        Err(failed("laboratory_quarantined", detail, false))
    }

    fn require_read_authority(&self) -> Result<(), LaboratoryError> {
        let permissions = self.input.conversation.effective_permissions();
        if permissions.allows(PermissionCapability::Read) {
            Ok(())
        } else {
            Err(rejected("laboratory_authority", "current authority does not permit reads"))
        }
    }

    fn require_effect_authority(&self) -> Result<(), LaboratoryError> {
        let permissions = self.input.conversation.effective_permissions();
        if permissions.allows(PermissionCapability::Read)
            && permissions.allows(PermissionCapability::Write)
            && permissions.allows(PermissionCapability::Process)
        {
            Ok(())
        } else {
            Err(rejected(
                "laboratory_authority",
                "current authority does not permit disposable source/process effects",
            ))
        }
    }
}

fn prepare_complete_source(
    lab: &mut StoredLaboratory,
    request: &ModuleLaboratoryLifecycleRequest,
    private: &Path,
    broker: &scoped_source::Broker<'_>,
    cancelled: &Arc<AtomicBool>,
) -> Result<ModuleLaboratoryReceipt, LaboratoryError> {
    let source_root = private.join("source");
    let tree = broker
        .materialize_tree(&lab.source, &source_root, 100_000, 2 * 1024 * 1024 * 1024)
        .map_err(source_error)?;
    if tree.snapshot_digest != lab.status.handle.source_snapshot {
        return Err(failed(
            "laboratory_source",
            "materialized source tree did not retain the laboratory snapshot identity",
            false,
        ));
    }
    for protected in [".git", ".peritus", ".crosslink"] {
        fs::create_dir_all(source_root.join(protected)).map_err(io_error)?;
    }
    let cargo_dependencies = prepare_cargo_dependencies(&source_root, cancelled)?;
    let secret = create_private_secret(private)?;
    lab.source_digest = tree.snapshot_digest;
    lab.secret_digest = digest(&secret);
    lab.source_tree_files = tree.file_count;
    lab.source_tree_bytes = tree.total_bytes;
    lab.status.phase = ModuleLaboratoryPhase::Prepared;
    lab.status.telemetry.state_digest =
        digest(format!("{}\0{}", tree.file_count, tree.total_bytes).as_bytes());
    lab.status.detail = format!(
        "Complete immutable source copy prepared: {} files / {} bytes.",
        tree.file_count, tree.total_bytes,
    );
    let launch_candidates = discover_source_candidates(&source_root)?;
    let receipt = receipt(
        lab,
        "prepare",
        digest(request.laboratory_id.as_bytes()),
        json!({
            "complete_source_copy": true,
            "files": tree.file_count,
            "bytes": tree.total_bytes,
            "supported_operation": PROCESS_OPERATION,
            "launch_candidates": launch_candidates,
            "cargo_dependencies": cargo_dependencies,
        }),
        false,
        None,
    );
    lab.receipts.push(receipt.clone());
    lab.status.receipts = count(&lab.receipts)?;
    Ok(receipt)
}

#[derive(Deserialize)]
struct CargoLockfile {
    #[serde(default)]
    package: Vec<CargoLockedPackage>,
}

#[derive(Deserialize)]
struct CargoLockedPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

struct CargoPreparationSummary {
    compressed_bytes: u64,
    expanded_bytes: u64,
    files: u64,
    identity: String,
}

fn prepare_cargo_dependencies(
    source_root: &Path,
    cancelled: &Arc<AtomicBool>,
) -> Result<Value, LaboratoryError> {
    let lock_path = source_root.join("Cargo.lock");
    if !lock_path.is_file() {
        return Ok(json!({"kind": "not_applicable"}));
    }
    let packages = cargo_locked_packages(&lock_path)?;
    if packages.is_empty() {
        return Ok(json!({"kind": "cargo_vendor_v1", "packages": 0}));
    }
    let partial = source_root.join(".peritus-vendor.partial");
    let vendor = source_root.join(".peritus-vendor");
    if partial.exists()
        || vendor.exists()
        || [".peritus-cargo-home", ".peritus-build-target", ".peritus-lab-home", ".peritus-lab-tmp"]
            .into_iter()
            .any(|path| source_root.join(path).exists())
    {
        return Err(rejected(
            "laboratory_dependencies",
            "source uses a reserved Peritus dependency-preparation path",
        ));
    }
    fs::create_dir(&partial).map_err(io_error)?;
    let prepared = (|| {
        let summary = fetch_cargo_packages(&partial, &packages, cancelled)?;
        fs::rename(&partial, &vendor).map_err(io_error)?;
        write_cargo_vendor_config(source_root, &vendor)?;
        Ok(json!({
            "kind": "cargo_vendor_v1",
            "packages": packages.len(),
            "compressed_bytes": summary.compressed_bytes,
            "expanded_bytes": summary.expanded_bytes,
            "files": summary.files,
            "identity": summary.identity,
            "network_policy": "public_https_crates_io_only; lockfile_sha256_required",
        }))
    })();
    if prepared.is_err() {
        let _ = fs::remove_dir_all(&partial);
        let _ = fs::remove_dir_all(&vendor);
        let _ = fs::remove_dir_all(source_root.join(".peritus-cargo-home"));
    }
    prepared
}

fn cargo_locked_packages(path: &Path) -> Result<Vec<CargoLockedPackage>, LaboratoryError> {
    let lock: CargoLockfile = toml::from_str(
        &fs::read_to_string(path)
            .map_err(|error| failed("laboratory_dependencies", &error.to_string(), true))?,
    )
    .map_err(|error| failed("laboratory_dependencies", &error.to_string(), false))?;
    let mut packages =
        lock.package.into_iter().filter(|package| package.source.is_some()).collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        (&left.name, &left.version, &left.source).cmp(&(&right.name, &right.version, &right.source))
    });
    if packages.len() > CARGO_DEPENDENCY_PACKAGES {
        return Err(rejected(
            "laboratory_dependencies",
            "Cargo.lock exceeds the bounded dependency package count",
        ));
    }
    for package in &packages {
        validate_cargo_package(package)?;
    }
    Ok(packages)
}

fn validate_cargo_package(package: &CargoLockedPackage) -> Result<(), LaboratoryError> {
    if package.source.as_deref() != Some("registry+https://github.com/rust-lang/crates.io-index") {
        return Err(rejected(
            "laboratory_dependencies",
            &format!(
                "Cargo dependency {} {} uses an unsupported non-crates.io source",
                package.name, package.version
            ),
        ));
    }
    if !cargo_component(&package.name)
        || !cargo_version(&package.version)
        || !package.checksum.as_deref().is_some_and(lower_sha256)
    {
        return Err(rejected(
            "laboratory_dependencies",
            "Cargo.lock contains an invalid registry dependency identity or checksum",
        ));
    }
    Ok(())
}

fn fetch_cargo_packages(
    destination: &Path,
    packages: &[CargoLockedPackage],
    cancelled: &Arc<AtomicBool>,
) -> Result<CargoPreparationSummary, LaboratoryError> {
    thread::scope(|scope| {
        scope
            .spawn(|| fetch_cargo_packages_on_worker(destination, packages, cancelled))
            .join()
            .map_err(|_| {
                failed("laboratory_dependencies", "dependency preparation worker panicked", true)
            })?
    })
}

fn fetch_cargo_packages_on_worker(
    destination: &Path,
    packages: &[CargoLockedPackage],
    cancelled: &Arc<AtomicBool>,
) -> Result<CargoPreparationSummary, LaboratoryError> {
    let mut compressed_bytes = 0_u64;
    let mut expanded_bytes = 0_u64;
    let mut files = 0_u64;
    let mut identity = Sha256::new();
    let mut fetcher = preparation::Fetcher::default();
    for package in packages {
        if cancelled.load(Ordering::Acquire) {
            return Err(failed(
                "laboratory_dependencies",
                "dependency preparation was cancelled",
                true,
            ));
        }
        let checksum = package.checksum.as_deref().ok_or_else(|| {
            rejected("laboratory_dependencies", "registry dependency has no checksum")
        })?;
        let remaining = CARGO_DEPENDENCY_TOTAL_BYTES.saturating_sub(compressed_bytes);
        if remaining == 0 {
            return Err(rejected(
                "laboratory_dependencies",
                "Cargo dependency archives exceed the bounded transfer size",
            ));
        }
        let specification = preparation::Specification {
            url: format!(
                "https://static.crates.io/crates/{0}/{0}-{1}.crate",
                package.name, package.version
            ),
            expected_sha256: Some(checksum.into()),
            maximum_bytes: remaining.min(CARGO_DEPENDENCY_ARCHIVE_BYTES),
        };
        let completed =
            fetcher.fetch(&specification, cancelled).map_err(cargo_preparation_error)?;
        compressed_bytes = compressed_bytes
            .checked_add(u64::try_from(completed.bytes.len()).map_err(|_| {
                rejected("laboratory_dependencies", "dependency archive is too large")
            })?)
            .ok_or_else(|| {
                rejected("laboratory_dependencies", "dependency byte count overflowed")
            })?;
        extract_crate_archive(
            &completed.bytes,
            &destination.join(format!("{}-{}", package.name, package.version)),
            &package.name,
            &package.version,
            checksum,
            &mut files,
            &mut expanded_bytes,
            cancelled,
        )?;
        for value in
            [package.name.as_str(), package.version.as_str(), checksum, completed.digest.as_str()]
        {
            identity.update(value.as_bytes());
            identity.update([0]);
        }
    }
    Ok(CargoPreparationSummary {
        compressed_bytes,
        expanded_bytes,
        files,
        identity: hex(&identity.finalize()),
    })
}

fn write_cargo_vendor_config(source_root: &Path, vendor: &Path) -> Result<(), LaboratoryError> {
    let cargo_home = source_root.join(".peritus-cargo-home");
    fs::create_dir(&cargo_home).map_err(io_error)?;
    let vendor_text = vendor
        .to_str()
        .ok_or_else(|| rejected("laboratory_dependencies", "vendor path is not valid UTF-8"))?;
    let quoted_vendor = toml::Value::String(vendor_text.into()).to_string();
    let config = format!(
        "[source.crates-io]\nreplace-with = \"peritus-vendored-sources\"\n\n[source.peritus-vendored-sources]\ndirectory = {quoted_vendor}\n\n[net]\noffline = true\n"
    );
    write_private(&cargo_home.join("config.toml"), config.as_bytes())
}

#[allow(clippy::too_many_arguments)]
fn extract_crate_archive(
    bytes: &[u8],
    destination: &Path,
    name: &str,
    version: &str,
    package_checksum: &str,
    file_count: &mut u64,
    expanded_bytes: &mut u64,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), LaboratoryError> {
    fs::create_dir(destination).map_err(io_error)?;
    let expected_root = format!("{name}-{version}");
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut files = BTreeMap::new();
    for entry in archive.entries().map_err(|error| {
        failed("laboratory_dependencies", &format!("read crate archive: {error}"), false)
    })? {
        if cancelled.load(Ordering::Acquire) {
            return Err(failed(
                "laboratory_dependencies",
                "dependency extraction was cancelled",
                true,
            ));
        }
        let mut entry = entry.map_err(|error| {
            failed("laboratory_dependencies", &format!("read crate entry: {error}"), false)
        })?;
        let archived = entry
            .path()
            .map_err(|error| {
                failed("laboratory_dependencies", &format!("read crate path: {error}"), false)
            })?
            .into_owned();
        let mut components = archived.components();
        let root = components.next().ok_or_else(|| {
            rejected("laboratory_dependencies", "crate archive entry has no package root")
        })?;
        if !matches!(root, Component::Normal(value) if value == expected_root.as_str()) {
            return Err(rejected(
                "laboratory_dependencies",
                "crate archive entry has an unexpected package root",
            ));
        }
        let relative = components.as_path();
        if relative.as_os_str().is_empty() {
            continue;
        }
        if !relative.components().all(|part| matches!(part, Component::Normal(_))) {
            return Err(rejected(
                "laboratory_dependencies",
                "crate archive entry escapes its package root",
            ));
        }
        let target = destination.join(relative);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(io_error)?;
            continue;
        }
        if !kind.is_file() {
            return Err(rejected(
                "laboratory_dependencies",
                "crate archive contains a link or special file",
            ));
        }
        *file_count = file_count.checked_add(1).ok_or_else(|| {
            rejected("laboratory_dependencies", "dependency file count overflowed")
        })?;
        *expanded_bytes = expanded_bytes.checked_add(entry.size()).ok_or_else(|| {
            rejected("laboratory_dependencies", "dependency byte count overflowed")
        })?;
        if *file_count > CARGO_DEPENDENCY_FILES || *expanded_bytes > CARGO_DEPENDENCY_EXPANDED_BYTES
        {
            return Err(rejected(
                "laboratory_dependencies",
                "expanded Cargo dependencies exceed the bounded file or byte count",
            ));
        }
        let relative_text = relative
            .to_str()
            .ok_or_else(|| rejected("laboratory_dependencies", "crate path is not valid UTF-8"))?;
        let expected_size = entry.size();
        files.insert(
            relative_text.replace('\\', "/"),
            write_dependency_file(&mut entry, &target, expected_size)?,
        );
    }
    let checksum = serde_json::to_vec(&json!({
        "files": files,
        "package": package_checksum,
    }))
    .map_err(json_error)?;
    write_private(&destination.join(".cargo-checksum.json"), &checksum)
}

fn write_dependency_file(
    input: &mut impl std::io::Read,
    target: &Path,
    expected_size: u64,
) -> Result<String, LaboratoryError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut output =
        fs::OpenOptions::new().write(true).create_new(true).open(target).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut copied = 0_u64;
    let mut chunk = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = input.read(&mut chunk).map_err(io_error)?;
        if count == 0 {
            break;
        }
        output.write_all(&chunk[..count]).map_err(io_error)?;
        hash.update(&chunk[..count]);
        copied = copied
            .checked_add(u64::try_from(count).map_err(|_| {
                rejected("laboratory_dependencies", "dependency entry is too large")
            })?)
            .ok_or_else(|| {
                rejected("laboratory_dependencies", "dependency entry size overflowed")
            })?;
    }
    if copied != expected_size {
        return Err(rejected(
            "laboratory_dependencies",
            "crate archive entry ended before its declared size",
        ));
    }
    output.sync_all().map_err(io_error)?;
    Ok(hex(&hash.finalize()))
}

fn cargo_preparation_error(error: preparation::Failure) -> LaboratoryError {
    match error {
        preparation::Failure::Cancelled => {
            failed("laboratory_dependencies", "dependency download was cancelled", true)
        }
        preparation::Failure::Rejected(detail) => rejected("laboratory_dependencies", &detail),
        preparation::Failure::Infrastructure(detail) => {
            failed("laboratory_dependencies", &detail, true)
        }
    }
}

fn cargo_component(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn cargo_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_'))
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn create_private_secret(private: &Path) -> Result<[u8; 32], LaboratoryError> {
    let mut secret = [0_u8; 32];
    getrandom::fill(&mut secret).map_err(|error| {
        failed("laboratory_secret", &format!("secret generation failed: {error}"), true)
    })?;
    write_private(&private.join("secret"), &secret)?;
    Ok(secret)
}

fn read_process_output(
    control: &ProcessControl,
    stream: OutputStream,
) -> Result<Vec<u8>, LaboratoryError> {
    control
        .spooled_stream_range(stream, 0, usize::try_from(OUTPUT_BYTES).unwrap_or(usize::MAX))
        .map(|(_, bytes)| bytes)
        .map_err(process_error)
}

fn source_process_result(
    input: &ModuleLaboratoryProcessInput,
    resources: ProcessResourcePolicy,
    observed: &SourceProcessObservation,
) -> SourceProcessResult {
    let exit_code = match observed.terminal.os_exit() {
        OsExitObservation::Code(code) => Some(*code),
        _ => None,
    };
    let exit_matched = exit_code.is_some_and(|code| input.expected_exit_codes.contains(&code));
    let stdout_matched = input
        .stdout_contains
        .iter()
        .all(|fragment| contains(&observed.stdout, fragment.as_bytes()));
    let stderr_matched = input
        .stderr_contains
        .iter()
        .all(|fragment| contains(&observed.stderr, fragment.as_bytes()));
    let cleanup =
        observed.terminal.tree_cleanup_complete() && observed.terminal.support_tasks_joined();
    let oracle_matched = exit_matched && stdout_matched && stderr_matched && cleanup;
    let output_bytes = u64::try_from(observed.stdout.len().saturating_add(observed.stderr.len()))
        .unwrap_or(u64::MAX);
    let observation = json!({
        "runtime": input.runtime,
        "entrypoint": input.entrypoint,
        "arguments": input.arguments,
        "environment_names": input.environment.keys().collect::<Vec<_>>(),
        "stdin_bytes": input.stdin.len(),
        "wall_millis": input.wall_millis,
        "effective_limits": {
            "memory_bytes": resources.memory_bytes(),
            "storage_bytes": resources.disk_bytes(),
            "cpu_millis": resources.cpu_millis().unwrap_or(input.wall_millis),
            "wall_millis": resources.wall_millis().unwrap_or(input.wall_millis),
            "processes": resources.process_count(),
        },
        "terminal_disposition": format!("{:?}", observed.terminal.disposition()).to_ascii_lowercase(),
        "exit_code": exit_code,
        "stdout": String::from_utf8_lossy(&observed.stdout),
        "stderr": String::from_utf8_lossy(&observed.stderr),
        "stdout_digest": digest(&observed.stdout),
        "stderr_digest": digest(&observed.stderr),
        "expected_exit_codes": input.expected_exit_codes,
        "exit_oracle_matched": exit_matched,
        "stdout_oracle_matched": stdout_matched,
        "stderr_oracle_matched": stderr_matched,
        "oracle_matched": oracle_matched,
        "cleanup_complete": cleanup,
        "network_policy": "deny_all",
        "execution_plan_digest": observed.execution_plan_digest,
    });
    SourceProcessResult { observation, oracle_matched, cleanup, output_bytes }
}

struct PreparedProcess {
    plan: ExecutionPlan,
    sandbox: peritus_sandbox::CheckedSandboxPlan,
    admission: peritus_sandbox::BackendAdmission,
    backend: LinuxBackend,
}

struct ResolvedSourceCommand {
    executable: PathBuf,
    arguments: Vec<String>,
}

fn prepare_source_process(
    config: &crate::NativeModuleJobConfiguration,
    identity: &peritus_process::ExecutionIdentity,
    work: &Path,
    command: &ResolvedSourceCommand,
    input: &ModuleLaboratoryProcessInput,
    resources: ProcessResourcePolicy,
    features: SourceProcessFeatures<'_>,
) -> Result<PreparedProcess, LaboratoryError> {
    let effective_environment = source_environment(work, &command.executable, input)?;
    let backend = LinuxBackend::new(
        LinuxBackendConfig::new(
            work.to_path_buf(),
            Vec::new(),
            config.bubblewrap.clone(),
            config.linux_helper.clone(),
            config.cgroup_root.clone(),
            None,
        )
        .map_err(|error| linux_error(&error))?
        .with_private_filesystem(),
    )
    .map_err(|error| linux_error(&error))?;
    let environment_names = effective_environment
        .keys()
        .map(|name| EnvironmentName::new(name.clone()).map_err(sandbox_error))
        .collect::<Result<Vec<_>, _>>()?;
    let sandbox = compile_source_process(
        identity,
        work,
        &command.executable,
        resources,
        &environment_names,
        !input.stdin.is_empty(),
        features,
    )?;
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
    let variables = effective_environment
        .iter()
        .map(|(name, value)| EnvironmentVariable::new(name.clone(), value.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(process_error)?;
    let command = CommandSpec::new(
        command
            .executable
            .to_str()
            .ok_or_else(|| rejected("laboratory_runtime", "resolved runtime path is not UTF-8"))?,
        command.arguments.clone(),
    )
    .map_err(process_error)?;
    let output = OutputPolicy::new(
        4_096,
        64 * 1024,
        OUTPUT_BYTES,
        8_192,
        OUTPUT_BYTES,
        OUTPUT_BYTES,
        OUTPUT_BYTES,
        OutputOverflowAction::Terminate,
    )
    .map_err(process_error)?;
    let stdin = if input.stdin.is_empty() {
        StdinPolicy::Closed
    } else {
        StdinPolicy::bounded(1024 * 1024, 1024 * 1024).map_err(process_error)?
    };
    let plan = ExecutionPlan::new(
        *identity,
        command,
        working,
        EnvironmentPlan::cleared(variables).map_err(process_error)?,
        IoMode::Pipes,
        stdin,
        output,
        DeadlinePolicy::new(Some(input.wall_millis), GracefulAction::Terminate, 250, 4_000)
            .map_err(process_error)?,
        resources,
        &sandbox,
        &admission,
    )
    .map_err(process_error)?;
    Ok(PreparedProcess { plan, sandbox, admission, backend })
}

fn worker_resources(
    request: &ModuleWorkerRequest,
) -> Result<ProcessResourcePolicy, LaboratoryError> {
    ProcessResourcePolicy::new(
        Some(request.limits.wall_millis),
        Some(request.limits.wall_millis),
        request.limits.memory_bytes,
        request.limits.storage_bytes,
        Some(OUTPUT_BYTES),
        u64::from(request.limits.processes),
        u64::from(request.limits.open_handles),
        1,
    )
    .map_err(process_error)
}

fn worker_private_directory() -> Result<tempfile::TempDir, LaboratoryError> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".cache"))
        })
        .ok_or_else(|| {
            failed(
                "worker_runtime",
                "a private user cache is unavailable for disposable module workers",
                true,
            )
        })?;
    let root = cache.join("peritus/jobs");
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(&root).map_err(io_error)?;
    let metadata = fs::symlink_metadata(&root).map_err(io_error)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failed(
            "worker_runtime",
            "the private module-worker cache is not a regular directory",
            false,
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
    }
    tempfile::Builder::new().prefix("w-").tempdir_in(root).map_err(io_error)
}

fn worker_process_input(
    request: &ModuleWorkerRequest,
    environment: BTreeMap<String, String>,
) -> Result<ModuleLaboratoryProcessInput, LaboratoryError> {
    Ok(ModuleLaboratoryProcessInput {
        schema: SCHEMA,
        runtime: "direct".into(),
        entrypoint: String::new(),
        arguments: vec!["worker".into(), request.worker.clone()],
        stdin: serde_json::to_vec(&request.input).map_err(json_error)?,
        environment,
        wall_millis: request.limits.wall_millis,
        expected_exit_codes: vec![0],
        stdout_contains: Vec::new(),
        stderr_contains: Vec::new(),
    })
}

fn source_environment(
    work: &Path,
    executable: &Path,
    input: &ModuleLaboratoryProcessInput,
) -> Result<BTreeMap<String, String>, LaboratoryError> {
    let private_home = work.join(".peritus-lab-home");
    fs::create_dir_all(&private_home).map_err(io_error)?;
    let mut environment = input.environment.clone();
    environment.insert("HOME".into(), private_home.to_string_lossy().into_owned());
    // The Linux backend replaces `/tmp` with a fresh mount for every process. Use that short
    // namespace path instead of a workspace-derived path so runtimes with AF_UNIX path limits
    // (notably Chromium) retain their normal socket-name headroom without sharing host scratch.
    environment.insert("TMPDIR".into(), "/tmp".into());
    let runtime_directory = executable.parent().ok_or_else(|| {
        rejected("laboratory_runtime", "resolved runtime has no parent directory")
    })?;
    environment.insert(
        "PATH".into(),
        format!("{}:/usr/local/bin:/usr/bin:/bin", runtime_directory.to_string_lossy()),
    );
    if input.runtime == "cargo" {
        let cargo_home = work.join(".peritus-cargo-home");
        fs::create_dir_all(&cargo_home).map_err(io_error)?;
        environment.insert("CARGO_HOME".into(), cargo_home.to_string_lossy().into_owned());
        let target = work.join(".peritus-build-target");
        fs::create_dir_all(&target).map_err(io_error)?;
        environment.insert("CARGO_TARGET_DIR".into(), target.to_string_lossy().into_owned());
    }
    Ok(environment)
}

fn source_environment_names(input: &ModuleLaboratoryProcessInput) -> BTreeSet<String> {
    let mut names = input.environment.keys().cloned().collect::<BTreeSet<_>>();
    names.extend(["HOME".into(), "TMPDIR".into(), "PATH".into()]);
    if input.runtime == "cargo" {
        names.insert("CARGO_HOME".into());
        names.insert("CARGO_TARGET_DIR".into());
    }
    names
}

fn source_resources(
    input: &ModuleLaboratoryProcessInput,
) -> Result<ProcessResourcePolicy, LaboratoryError> {
    let disk_bytes =
        if input.runtime == "cargo" { 16 * 1024 * 1024 * 1024 } else { 2 * 1024 * 1024 * 1024 };
    ProcessResourcePolicy::new(
        Some(input.wall_millis),
        Some(input.wall_millis),
        source_memory_bytes(input),
        disk_bytes,
        Some(OUTPUT_BYTES),
        64,
        8_192,
        1,
    )
    .map_err(process_error)
}

fn source_memory_bytes(input: &ModuleLaboratoryProcessInput) -> u64 {
    if input.runtime != "cargo" {
        return SOURCE_PROCESS_MEMORY_BYTES;
    }

    #[cfg(target_os = "linux")]
    if let Ok(memory) = fs::read_to_string("/proc/meminfo")
        && let Some(available_kib) = memory.lines().find_map(|line| {
            line.strip_prefix("MemAvailable:")
                .and_then(|value| value.split_ascii_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
    {
        let available_bytes = available_kib.saturating_mul(1024);
        return available_bytes
            .saturating_sub(CARGO_PROCESS_MEMORY_RESERVE_BYTES)
            .clamp(SOURCE_PROCESS_MEMORY_BYTES, CARGO_PROCESS_MEMORY_CAP_BYTES);
    }

    SOURCE_PROCESS_MEMORY_BYTES
}

fn compile_source_process(
    identity: &peritus_process::ExecutionIdentity,
    work: &Path,
    executable_path: &Path,
    resources: ProcessResourcePolicy,
    environment_names: &[EnvironmentName],
    stdin_enabled: bool,
    features: SourceProcessFeatures<'_>,
) -> Result<peritus_sandbox::CheckedSandboxPlan, LaboratoryError> {
    let executable = sandbox_path(executable_path)?;
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
    let descendants = u32::try_from(resources.process_count().saturating_sub(1)).map_err(|_| {
        failed("laboratory_runtime", "process-count limit is not representable", false)
    })?;
    let output = ResourceQuantity::new(OUTPUT_BYTES);
    let events = ResourceQuantity::new(8_192);
    let input_permission =
        if stdin_enabled { InputPermission::Allowed } else { InputPermission::Denied };
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        input_permission,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        TerminalLimits::new(None, events, output).map_err(sandbox_error)?,
    )
    .map_err(sandbox_error)?;
    let filesystem = source_process_filesystem(work, executable_path, features)?;
    let null_device = sandbox_path(Path::new("/dev/null"))?;
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
        EnvironmentContract::new(EnvironmentMode::Cleared, environment_names.to_vec())
            .map_err(sandbox_error)?,
        NetworkContract::deny_all(),
        SecretContract::deny_all(),
        limits,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![
            FileRequirement::new(executable.clone(), FileOperation::Execute),
            FileRequirement::new(null_device.clone(), FileOperation::Read),
            FileRequirement::new(null_device, FileOperation::Write),
        ],
        ProcessRequirements::new(executable, descendants, true),
        EnvironmentRequirements::new(Vec::new(), environment_names.to_vec())
            .map_err(sandbox_error)?,
        Vec::new(),
        Vec::new(),
        limits,
        TerminalRequirements::new(
            TerminalMode::Pipes,
            input_permission,
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

fn source_process_filesystem(
    work: &Path,
    executable_path: &Path,
    features: SourceProcessFeatures<'_>,
) -> Result<FilesystemContract, LaboratoryError> {
    let execute = FileOperationSet::from_operations([
        FileOperation::Read,
        FileOperation::Discover,
        FileOperation::Metadata,
        FileOperation::Execute,
    ]);
    let mut rules = vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            sandbox_path(executable_path)?,
            PathScope::Exact,
            execute,
        )
        .map_err(sandbox_error)?,
        FilesystemRule::new(
            RuleEffect::Allow,
            sandbox_path(work)?,
            PathScope::Descendants,
            FileOperationSet::from_operations([
                FileOperation::Read,
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Execute,
                FileOperation::Create,
                FileOperation::Write,
                FileOperation::Remove,
            ]),
        )
        .map_err(sandbox_error)?,
        // Child-process APIs routinely open the null device while setting up inherited standard
        // streams. Without this exact device grant, a permitted compiler or test runner can be
        // reported as "never executed" even though its executable path is admitted.
        FilesystemRule::new(
            RuleEffect::Allow,
            sandbox_path(Path::new("/dev/null"))?,
            PathScope::Exact,
            FileOperationSet::from_operations([
                FileOperation::Read,
                FileOperation::Write,
                FileOperation::Metadata,
            ]),
        )
        .map_err(sandbox_error)?,
        private_tmp_rule()?,
    ];
    for runtime in [
        // Distribution toolchain links such as Fedora's /usr/bin/ld resolve through this
        // non-secret, read-only system directory. Without it, an admitted compiler sees a broken
        // linker path inside the private mount namespace.
        "/etc/alternatives",
        "/usr/bin",
        "/usr/lib",
        "/usr/lib64",
        "/usr/libexec",
        "/usr/local/bin",
        "/usr/local/lib",
        "/usr/share",
        "/bin",
        "/lib",
        "/lib64",
    ] {
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
    rules.extend(source_header_rules()?);
    for root in features.roots {
        rules.push(
            FilesystemRule::new(
                RuleEffect::Allow,
                sandbox_path(root)?,
                PathScope::Descendants,
                execute,
            )
            .map_err(sandbox_error)?,
        );
    }
    if features.private_proc {
        rules.push(private_proc_rule()?);
    }
    if features.random_device {
        rules.push(random_device_rule()?);
    }
    if executable_path.to_string_lossy().contains("/.rustup/toolchains/")
        && let Some(toolchain_root) = executable_path.parent().and_then(Path::parent)
    {
        rules.push(
            FilesystemRule::new(
                RuleEffect::Allow,
                sandbox_path(toolchain_root)?,
                PathScope::Descendants,
                execute,
            )
            .map_err(sandbox_error)?,
        );
    }
    FilesystemContract::new(rules).map_err(sandbox_error)
}

fn source_header_rules() -> Result<Vec<FilesystemRule>, LaboratoryError> {
    let read = FileOperationSet::from_operations([
        FileOperation::Read,
        FileOperation::Discover,
        FileOperation::Metadata,
    ]);
    ["/usr/include", "/usr/local/include"]
        .into_iter()
        .filter_map(|headers| Path::new(headers).canonicalize().ok())
        .map(|headers| {
            FilesystemRule::new(
                RuleEffect::Allow,
                sandbox_path(&headers)?,
                PathScope::Descendants,
                read,
            )
            .map_err(sandbox_error)
        })
        .collect()
}

fn private_tmp_rule() -> Result<FilesystemRule, LaboratoryError> {
    // `/tmp` is a fresh tmpfs inside this process's private mount namespace. Granting scratch
    // operations here does not expose the host's shared temporary directory.
    FilesystemRule::new(
        RuleEffect::Allow,
        sandbox_path(Path::new("/tmp"))?,
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
    .map_err(sandbox_error)
}

fn private_proc_rule() -> Result<FilesystemRule, LaboratoryError> {
    // The Linux backend mounts a fresh procfs after entering the worker's private PID namespace.
    // Browser engines need that isolated view for process discovery and child startup; it
    // contains no host process tree and remains read-only under Landlock.
    FilesystemRule::new(
        RuleEffect::Allow,
        sandbox_path(Path::new("/proc"))?,
        PathScope::Descendants,
        FileOperationSet::from_operations([
            FileOperation::Read,
            FileOperation::Discover,
            FileOperation::Metadata,
        ]),
    )
    .map_err(sandbox_error)
}

fn random_device_rule() -> Result<FilesystemRule, LaboratoryError> {
    // Bubblewrap's fresh `/dev` contains its own urandom device. Some browser subprocesses use
    // that standard interface even when the kernel `getrandom` syscall is available.
    FilesystemRule::new(
        RuleEffect::Allow,
        sandbox_path(Path::new("/dev/urandom"))?,
        PathScope::Exact,
        FileOperationSet::from_operations([FileOperation::Read, FileOperation::Metadata]),
    )
    .map_err(sandbox_error)
}

fn source_recipe() -> ModuleLaboratoryRecipe {
    ModuleLaboratoryRecipe {
        schema: SCHEMA,
        recipe_id: SOURCE_RECIPE.into(),
        version: "1.0.0".into(),
        platform: ModuleHostPlatform {
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
        },
        source_roles: vec!["complete_immutable_source_tree".into()],
        isolation_backend: format!(
            "{}@{}",
            peritus_sandbox_linux::BACKEND_NAME,
            peritus_sandbox_linux::BACKEND_VERSION
        ),
        interface: "literal argv + bounded stdin/stdout/stderr".into(),
    }
}

fn handles(
    laboratory_id: &str,
    scope_revision: &str,
    snapshot: &str,
    recipe_id: &str,
) -> ModuleLaboratoryHandle {
    let opaque =
        |kind: &str| digest(format!("peritus.lab.handle.v1\0{kind}\0{laboratory_id}").as_bytes());
    ModuleLaboratoryHandle {
        schema: SCHEMA,
        laboratory_id: laboratory_id.into(),
        scope_revision: scope_revision.into(),
        source_snapshot: snapshot.into(),
        recipe_id: recipe_id.into(),
        application_service: opaque("source-copy"),
        backing_service: opaque("none"),
        endpoint_handle: opaque("process-endpoint"),
        identity_handle: opaque("none"),
        secret_handle: opaque("output-canary"),
    }
}

const fn empty_telemetry() -> ModuleLaboratoryTelemetry {
    ModuleLaboratoryTelemetry {
        requests: 0,
        input_bytes: 0,
        output_bytes: 0,
        state_digest: String::new(),
        host_loopback_blocked: false,
        credential_output_blocked: true,
    }
}

fn receipt(
    lab: &StoredLaboratory,
    operation: &str,
    request_digest: String,
    observation: Value,
    cleanup: bool,
    unchanged: Option<bool>,
) -> ModuleLaboratoryReceipt {
    ModuleLaboratoryReceipt {
        schema: SCHEMA,
        laboratory_id: lab.status.handle.laboratory_id.clone(),
        operation: operation.into(),
        request_digest,
        source_snapshot: lab.status.handle.source_snapshot.clone(),
        application_digest: lab.source_digest.clone(),
        service_digest: String::new(),
        isolation_backend: source_recipe().isolation_backend,
        network_policy: "private-network-namespace; deny-all; no-ambient-credentials".into(),
        observation,
        telemetry: lab.status.telemetry.clone(),
        process_cleanup_complete: cleanup,
        original_source_unchanged: unchanged,
    }
}

fn result(
    status: ModuleLaboratoryStatus,
    receipt: Option<ModuleLaboratoryReceipt>,
) -> Result<Value, LaboratoryError> {
    encode(ModuleLaboratoryResult { schema: SCHEMA, status, receipt })
}

fn contains_secret(bytes: &[u8], secret: &[u8]) -> bool {
    contains(bytes, secret) || contains(bytes, hex(secret).as_bytes())
}

fn source_tree_digest(root: &Path) -> Result<(String, u64, u64), LaboratoryError> {
    let mut paths = Vec::new();
    collect_source_files(root, root, &mut paths)?;
    paths.sort();
    let mut hasher = Sha256::new();
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    for relative in paths {
        let path = root.join(&relative);
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if !metadata.file_type().is_file() {
            return Err(failed(
                "laboratory_source",
                "disposable source tree contains a non-regular file",
                false,
            ));
        }
        files = files
            .checked_add(1)
            .ok_or_else(|| failed("laboratory_state", "source file count overflow", false))?;
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or_else(|| failed("laboratory_state", "source byte count overflow", false))?;
        if files > 100_000 || bytes > 2 * 1024 * 1024 * 1024 {
            return Err(failed(
                "laboratory_source",
                "disposable source tree exceeded retained snapshot bounds",
                false,
            ));
        }
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        hasher.update(metadata.len().to_le_bytes());
        let mut input = fs::File::open(&path).map_err(io_error)?;
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            let read = input.read(&mut chunk).map_err(io_error)?;
            if read == 0 {
                break;
            }
            hasher.update(&chunk[..read]);
        }
    }
    Ok((hex(&hasher.finalize()), files, bytes))
}

fn discover_source_candidates(root: &Path) -> Result<Vec<Value>, LaboratoryError> {
    let mut relative = Vec::new();
    collect_source_files(root, root, &mut relative)?;
    relative.sort();
    let names = relative.iter().filter_map(|path| path.to_str()).collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();
    push_source_candidate(
        &names,
        &mut candidates,
        "cargo",
        "Cargo.toml",
        "high",
        "root Rust package manifest",
    );
    push_source_candidate(
        &names,
        &mut candidates,
        "go",
        "go.mod",
        "high",
        "root Go module manifest",
    );
    if names.contains("package.json")
        && let Ok(bytes) = fs::read(root.join("package.json"))
        && let Ok(package) = serde_json::from_slice::<Value>(&bytes)
        && package
            .get("packageManager")
            .and_then(Value::as_str)
            .is_some_and(|manager| manager.starts_with("bun@"))
    {
        push_source_candidate(
            &names,
            &mut candidates,
            "bun",
            "package.json",
            "high",
            "package.json declares Bun as its package manager",
        );
    }
    for (runtime, entrypoint, reason) in [
        ("python3", "main.py", "conventional Python entry point"),
        ("python3", "__main__.py", "Python module entry point"),
        ("python3", "app.py", "conventional Python application entry point"),
        ("node", "index.js", "conventional Node entry point"),
        ("node", "main.js", "conventional Node entry point"),
        ("node", "server.js", "conventional Node server entry point"),
        ("bash", "run.sh", "conventional shell entry point"),
        ("bash", "start.sh", "conventional shell entry point"),
        ("ruby", "main.rb", "conventional Ruby entry point"),
        ("php", "index.php", "conventional PHP entry point"),
        ("perl", "main.pl", "conventional Perl entry point"),
        ("make", "Makefile", "root Make build entry point"),
    ] {
        push_source_candidate(&names, &mut candidates, runtime, entrypoint, "medium", reason);
    }
    if names.contains("package.json")
        && let Ok(bytes) = fs::read(root.join("package.json"))
        && let Ok(package) = serde_json::from_slice::<Value>(&bytes)
        && let Some(main) = package.get("main").and_then(Value::as_str)
    {
        push_source_candidate(
            &names,
            &mut candidates,
            "node",
            main,
            "high",
            "package.json main entry point",
        );
    }
    for path in &relative {
        if candidates.len() >= 32 {
            break;
        }
        let Some(text) = path.to_str() else { continue };
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let runtime = match extension {
            "py" => "python3",
            "js" | "mjs" | "cjs" => "node",
            "sh" => "bash",
            "rb" => "ruby",
            "php" => "php",
            "pl" => "perl",
            "jar" => "java",
            "csproj" => "dotnet",
            _ => continue,
        };
        push_source_candidate(
            &names,
            &mut candidates,
            runtime,
            text,
            "low",
            "language-compatible source entry point candidate",
        );
    }
    Ok(candidates)
}

fn push_source_candidate(
    names: &BTreeSet<&str>,
    candidates: &mut Vec<Value>,
    runtime: &str,
    entrypoint: &str,
    confidence: &str,
    reason: &str,
) {
    if names.contains(entrypoint)
        && !candidates
            .iter()
            .any(|value| value["runtime"] == runtime && value["entrypoint"] == entrypoint)
    {
        candidates.push(json!({
            "runtime": runtime,
            "entrypoint": entrypoint,
            "confidence": confidence,
            "reason": reason,
        }));
    }
}

fn collect_source_files(
    root: &Path,
    current: &Path,
    paths: &mut Vec<PathBuf>,
) -> Result<(), LaboratoryError> {
    for entry in fs::read_dir(current).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_error)?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| failed("laboratory_source", "source path escaped its root", false))?;
        if current == root
            && file_type.is_dir()
            && matches!(
                relative.to_str(),
                Some(
                    ".peritus-vendor"
                        | ".peritus-vendor.partial"
                        | ".peritus-cargo-home"
                        | ".peritus-build-target"
                        | ".peritus-lab-home"
                        | ".peritus-lab-tmp"
                )
            )
        {
            continue;
        }
        if file_type.is_symlink() {
            return Err(failed(
                "laboratory_source",
                "disposable source tree contains a symbolic link",
                false,
            ));
        }
        if file_type.is_dir() {
            collect_source_files(root, &path, paths)?;
        } else if file_type.is_file() {
            paths.push(relative.to_path_buf());
        } else {
            return Err(failed(
                "laboratory_source",
                "disposable source tree contains an unsupported special file",
                false,
            ));
        }
    }
    Ok(())
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

fn owned<'a>(
    index: &'a Index,
    id: &str,
    module_id: &str,
    module_digest: &str,
) -> Result<&'a StoredLaboratory, LaboratoryError> {
    let lab = index
        .laboratories
        .get(id)
        .ok_or_else(|| rejected("laboratory_unknown", "the exact laboratory is not retained"))?;
    require_owner(lab, module_id, module_digest)?;
    Ok(lab)
}

fn owned_mut<'a>(
    index: &'a mut Index,
    id: &str,
    module_id: &str,
    module_digest: &str,
) -> Result<&'a mut StoredLaboratory, LaboratoryError> {
    let lab = index
        .laboratories
        .get_mut(id)
        .ok_or_else(|| rejected("laboratory_unknown", "the exact laboratory is not retained"))?;
    require_owner(lab, module_id, module_digest)?;
    Ok(lab)
}

fn require_owner(
    lab: &StoredLaboratory,
    module_id: &str,
    module_digest: &str,
) -> Result<(), LaboratoryError> {
    if lab.module_id == module_id && lab.module_digest == module_digest {
        Ok(())
    } else {
        Err(rejected("laboratory_owner", "laboratory belongs to a different exact module package"))
    }
}

fn validate_lifecycle(request: &ModuleLaboratoryLifecycleRequest) -> Result<(), LaboratoryError> {
    validate_schema(request.schema)?;
    if valid_id(&request.laboratory_id) {
        Ok(())
    } else {
        Err(rejected("laboratory_identity", "invalid laboratory identity"))
    }
}

fn validate_process_input(input: &ModuleLaboratoryProcessInput) -> Result<(), LaboratoryError> {
    validate_schema(input.schema)?;
    validate_generic_source_path(&input.entrypoint)?;
    if !matches!(
        input.runtime.as_str(),
        "direct"
            | "python3"
            | "node"
            | "bun"
            | "bash"
            | "ruby"
            | "php"
            | "perl"
            | "cargo"
            | "go"
            | "java"
            | "dotnet"
            | "make"
    ) {
        return Err(rejected(
            "laboratory_runtime",
            "runtime must be direct, python3, node, bun, bash, ruby, php, perl, cargo, go, java, dotnet, or make",
        ));
    }
    if input.arguments.len() > 256
        || input.arguments.iter().any(|value| value.len() > 16 * 1024 || value.contains('\0'))
        || input.stdin.len() > 1024 * 1024
        || input.environment.len() > 64
        || !(100..=3_600_000).contains(&input.wall_millis)
        || input.environment.iter().any(|(name, value)| {
            let upper = name.to_ascii_uppercase();
            name.is_empty()
                || name.len() > 128
                || value.len() > 64 * 1024
                || value.contains('\0')
                || ["TOKEN", "PASSWORD", "SECRET", "CREDENTIAL", "API_KEY", "AUTH"]
                    .iter()
                    .any(|marker| upper.contains(marker))
        })
        || input.expected_exit_codes.is_empty()
        || input.expected_exit_codes.len() > 16
        || input.stdout_contains.len() > 32
        || input.stderr_contains.len() > 32
        || input
            .stdout_contains
            .iter()
            .chain(&input.stderr_contains)
            .any(|fragment| fragment.len() > 4_096 || fragment.contains('\0'))
    {
        return Err(rejected(
            "laboratory_input",
            "generic process input exceeds argv, stdin, environment, oracle, or credential-safety bounds",
        ));
    }
    Ok(())
}

fn validate_generic_source_path(path: &str) -> Result<(), LaboratoryError> {
    if path.is_empty()
        || path.len() > 4_096
        || path.contains('\0')
        || path.contains('\\')
        || Path::new(path).is_absolute()
        || !Path::new(path).components().all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(rejected(
            "laboratory_source",
            "entrypoint must be a canonical source-relative regular file",
        ));
    }
    Ok(())
}

fn validate_worker_request(request: &ModuleWorkerRequest) -> Result<(), LaboratoryError> {
    validate_schema(request.schema)?;
    let input_bytes = serde_json::to_vec(&request.input).map_err(json_error)?.len();
    if !valid_worker_name(&request.worker)
        || !(100..=3_600_000).contains(&request.limits.wall_millis)
        || !(16 * 1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&request.limits.memory_bytes)
        || !(1..=2 * 1024 * 1024 * 1024).contains(&request.limits.storage_bytes)
        || !(1..=8_192).contains(&request.limits.processes)
        || !(16..=4_096).contains(&request.limits.open_handles)
        || input_bytes > 1024 * 1024
        || request.runtime_features.len() > 8
        || request.runtime_features.iter().any(|feature| !valid_worker_name(feature))
    {
        return Err(rejected(
            "worker_request",
            "worker identity, input, or resource request exceeds the generic host bounds",
        ));
    }
    let unique = request.runtime_features.iter().collect::<BTreeSet<_>>();
    if unique.len() != request.runtime_features.len() {
        return Err(rejected("worker_request", "worker runtime features must be unique"));
    }
    Ok(())
}

fn worker_runtime_features(names: &[String]) -> Result<WorkerRuntimeFeatures, LaboratoryError> {
    let mut runtime = WorkerRuntimeFeatures::default();
    for name in names {
        if name == MODULE_WORKER_FEATURE_CARGO_DEPENDENCIES {
            runtime.prepare_cargo_dependencies = true;
            continue;
        }
        if name == MODULE_WORKER_FEATURE_BUN_RUNTIME {
            let bun = required_runtime("bun", &["/usr/bin/bun", "/usr/local/bin/bun"])?;
            let bun_directory = bun.parent().ok_or_else(|| {
                rejected("worker_runtime_feature", "the Bun executable has no runtime directory")
            })?;
            runtime
                .environment
                .insert("PERITUS_BUN_EXECUTABLE".into(), bun.to_string_lossy().into_owned());
            runtime.read_only_roots.push(bun_directory.to_path_buf());
            if let Some(cache) = std::env::var_os("BUN_INSTALL_CACHE_DIR")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("BUN_INSTALL")
                        .map(PathBuf::from)
                        .or_else(|| {
                            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".bun"))
                        })
                        .map(|root| root.join("install/cache"))
                })
                .and_then(|path| fs::canonicalize(path).ok())
                .filter(|path| path.is_dir())
            {
                runtime
                    .environment
                    .insert("BUN_INSTALL_CACHE_DIR".into(), cache.to_string_lossy().into_owned());
                runtime.read_only_roots.push(cache);
            }
            continue;
        }
        if name != MODULE_WORKER_FEATURE_BROWSER_AUTOMATION {
            return Err(rejected(
                "worker_runtime_feature",
                &format!("module worker runtime feature `{name}` is unavailable"),
            ));
        }
        peritus_tools_computer::ensure_setup().map_err(|error| {
            failed(
                "worker_runtime_feature",
                &format!("prepare managed browser automation runtime: {error}"),
                true,
            )
        })?;
        let python = peritus_tools_computer::python().map_err(|error| {
            failed("worker_runtime_feature", &format!("resolve managed Python: {error}"), true)
        })?;
        let cache = peritus_tools_computer::browser_cache().map_err(|error| {
            failed(
                "worker_runtime_feature",
                &format!("resolve managed browser cache: {error}"),
                true,
            )
        })?;
        if cache == Path::new("0") {
            return Err(rejected(
                "worker_runtime_feature",
                "PLAYWRIGHT_BROWSERS_PATH=0 cannot be projected into an isolated module worker",
            ));
        }
        let selected_runtime_root = python.parent().and_then(Path::parent).ok_or_else(|| {
            rejected("worker_runtime_feature", "managed Python has no runtime root")
        })?;
        let runtime_relative = python.strip_prefix(selected_runtime_root).map_err(|_| {
            rejected(
                "worker_runtime_feature",
                "managed Python is outside its selected runtime root",
            )
        })?;
        let runtime_root = fs::canonicalize(selected_runtime_root).map_err(io_error)?;
        // Runtime discovery may traverse an application-directory symlink (for example, an
        // isolated XDG root which deliberately reuses a prepared managed runtime). The private
        // filesystem projects the canonical directory, so publish the equivalent path through
        // that projection rather than an alias which is intentionally absent in the worker.
        let projected_python = runtime_root.join(runtime_relative);
        if !projected_python.is_file() {
            return Err(rejected(
                "worker_runtime_feature",
                "managed Python is absent from its canonical runtime projection",
            ));
        }
        let cache = fs::canonicalize(cache).map_err(io_error)?;
        runtime.environment.insert(
            "PERITUS_AUTOMATION_PYTHON".into(),
            projected_python.to_string_lossy().into_owned(),
        );
        runtime
            .environment
            .insert("PLAYWRIGHT_BROWSERS_PATH".into(), cache.to_string_lossy().into_owned());
        runtime.environment.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
        runtime.read_only_roots.extend([runtime_root, cache]);
        runtime.read_private_proc = true;
        runtime.read_random_device = true;
    }
    runtime.read_only_roots.sort();
    runtime.read_only_roots.dedup();
    Ok(runtime)
}

const fn terminal_cleanup_complete(terminal: &TerminalResult) -> bool {
    terminal.tree_cleanup_complete() && terminal.support_tasks_joined()
}

fn resolve_source_command(
    source_root: &Path,
    input: &ModuleLaboratoryProcessInput,
) -> Result<ResolvedSourceCommand, LaboratoryError> {
    let entrypoint = fs::canonicalize(source_root.join(&input.entrypoint)).map_err(|_| {
        rejected(
            "laboratory_entrypoint",
            "the selected entrypoint is absent from the complete immutable source copy",
        )
    })?;
    let source_root = fs::canonicalize(source_root).map_err(io_error)?;
    if !entrypoint.starts_with(&source_root) || !entrypoint.is_file() {
        return Err(rejected(
            "laboratory_entrypoint",
            "the selected entrypoint is not a regular file within the disposable source copy",
        ));
    }
    let (executable, mut arguments) = match input.runtime.as_str() {
        "direct" => (entrypoint, Vec::new()),
        "python3" => {
            (required_runtime("python3", &["/usr/bin/python3"])?, vec![path_text(&entrypoint)?])
        }
        "node" => (
            required_runtime("node", &["/usr/bin/node", "/usr/local/bin/node"])?,
            vec![path_text(&entrypoint)?],
        ),
        "bun" => {
            let executable = required_runtime("bun", &["/usr/bin/bun", "/usr/local/bin/bun"])?;
            let arguments = if input.entrypoint == "package.json" {
                vec!["run".into(), "start".into()]
            } else {
                vec![path_text(&entrypoint)?]
            };
            (executable, arguments)
        }
        "bash" => (
            required_runtime("bash", &["/usr/bin/bash", "/bin/bash"])?,
            vec![path_text(&entrypoint)?],
        ),
        "ruby" => (
            required_runtime("ruby", &["/usr/bin/ruby", "/usr/local/bin/ruby"])?,
            vec![path_text(&entrypoint)?],
        ),
        "php" => (
            required_runtime("php", &["/usr/bin/php", "/usr/local/bin/php"])?,
            vec![path_text(&entrypoint)?],
        ),
        "perl" => (required_runtime("perl", &["/usr/bin/perl"])?, vec![path_text(&entrypoint)?]),
        "cargo" => (
            required_runtime("cargo", &["/usr/bin/cargo", "/usr/local/bin/cargo"])?,
            vec![
                "--quiet".into(),
                "run".into(),
                "--locked".into(),
                "--offline".into(),
                "--jobs".into(),
                "2".into(),
                "--manifest-path".into(),
                path_text(&entrypoint)?,
                "--".into(),
            ],
        ),
        "go" => (
            required_runtime("go", &["/usr/bin/go", "/usr/local/go/bin/go"])?,
            vec![
                "run".into(),
                path_text(entrypoint.parent().ok_or_else(|| {
                    rejected("laboratory_entrypoint", "Go module manifest has no parent")
                })?)?,
            ],
        ),
        "java" => (
            required_runtime("java", &["/usr/bin/java", "/usr/local/bin/java"])?,
            vec!["-jar".into(), path_text(&entrypoint)?],
        ),
        "dotnet" => (
            required_runtime("dotnet", &["/usr/bin/dotnet", "/usr/local/bin/dotnet"])?,
            vec!["run".into(), "--project".into(), path_text(&entrypoint)?, "--".into()],
        ),
        "make" => (
            required_runtime("make", &["/usr/bin/make", "/usr/local/bin/make"])?,
            vec!["--file".into(), path_text(&entrypoint)?],
        ),
        _ => return Err(rejected("laboratory_runtime", "unsupported generic source runtime")),
    };
    arguments.extend(input.arguments.iter().cloned());
    Ok(ResolvedSourceCommand { executable, arguments })
}

fn required_runtime(label: &str, candidates: &[&str]) -> Result<PathBuf, LaboratoryError> {
    if let Some(path) = candidates
        .iter()
        .find_map(|candidate| fs::canonicalize(candidate).ok().filter(|path| path.is_file()))
    {
        return Ok(path);
    }
    if label == "cargo"
        && let Some(path) = rustup_toolchain_program("cargo")
    {
        return Ok(path);
    }
    if label == "bun"
        && let Some(path) = user_runtime_program("BUN_INSTALL", ".bun", "bun")
    {
        return Ok(path);
    }
    Err(rejected("laboratory_runtime", &format!("the host-owned {label} runtime is unavailable")))
}

fn user_runtime_program(environment: &str, default_root: &str, program: &str) -> Option<PathBuf> {
    let root = std::env::var_os(environment)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(default_root)))?;
    fs::canonicalize(root.join("bin").join(program)).ok().filter(|path| path.is_file())
}

fn rustup_toolchain_program(program: &str) -> Option<PathBuf> {
    let rustup = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".rustup")))?;
    let mut candidates = fs::read_dir(rustup.join("toolchains"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("bin").join(program))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next().and_then(|path| fs::canonicalize(path).ok())
}

fn path_text(path: &Path) -> Result<String, LaboratoryError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| rejected("laboratory_path", "resolved source path is not UTF-8"))
}

fn validate_schema(schema: u16) -> Result<(), LaboratoryError> {
    if schema == SCHEMA {
        Ok(())
    } else {
        Err(rejected("laboratory_schema", "laboratory requests require schema 1"))
    }
}

fn valid_id(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_worker_name(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'-' | b'_'))
        })
}

fn load(root: &Path) -> Result<Index, LaboratoryError> {
    match fs::read(root.join("index.json")) {
        Ok(bytes) => {
            let index: Index = serde_json::from_slice(&bytes).map_err(json_error)?;
            if index.schema != SCHEMA {
                return Err(failed(
                    "laboratory_state_schema",
                    "unsupported laboratory state schema",
                    false,
                ));
            }
            Ok(index)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Index::default()),
        Err(error) => Err(io_error(error)),
    }
}

fn save(root: &Path, index: &Index) -> Result<(), LaboratoryError> {
    fs::create_dir_all(root).map_err(io_error)?;
    let bytes = serde_json::to_vec_pretty(index).map_err(json_error)?;
    write_atomic(&root.join("index.json"), &bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), LaboratoryError> {
    let parent = path
        .parent()
        .ok_or_else(|| failed("laboratory_path", "state path has no parent", false))?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    temporary.write_all(bytes).map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary.persist(path).map_err(|error| io_error(error.error))?;
    Ok(())
}

fn module_live(module_id: &str, module_digest: &str) -> bool {
    Registry::for_user()
        .and_then(|registry| registry.load(module_id, true))
        .is_ok_and(|definition| definition.digest == module_digest)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), LaboratoryError> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn laboratory_directory(root: &Path, id: &str) -> PathBuf {
    root.join("laboratories").join(id)
}

fn sandbox_path(path: &Path) -> Result<SandboxPath, LaboratoryError> {
    SandboxPath::new(
        path.to_str().ok_or_else(|| rejected("laboratory_path", "laboratory path is not UTF-8"))?,
    )
    .map_err(sandbox_error)
}

fn count<T>(items: &[T]) -> Result<u64, LaboratoryError> {
    items
        .len()
        .try_into()
        .map_err(|_| failed("laboratory_state", "retained record count overflow", false))
}

fn encode<T: Serialize>(value: T) -> Result<Value, LaboratoryError> {
    serde_json::to_value(value).map_err(json_error)
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, LaboratoryError> {
    serde_json::from_value(value).map_err(|_| {
        rejected("laboratory_input", "laboratory input does not match contract version 1")
    })
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    crate::encode_hex(bytes)
}

fn rejected(code: &str, detail: &str) -> LaboratoryError {
    LaboratoryError::Rejected { code: code.into(), detail: detail.into() }
}

fn failed(code: &str, detail: &str, retryable: bool) -> LaboratoryError {
    LaboratoryError::Failed { code: code.into(), detail: detail.into(), retryable }
}

fn state_poisoned() -> LaboratoryError {
    failed("laboratory_state", "laboratory ownership state is poisoned", false)
}

fn source_error(error: scoped_source::BrokerError) -> LaboratoryError {
    match error {
        scoped_source::BrokerError::Rejected { code, detail } => {
            LaboratoryError::Rejected { code, detail }
        }
        scoped_source::BrokerError::Failed { code, detail, retryable } => {
            LaboratoryError::Failed { code, detail, retryable }
        }
    }
}

fn io_error(error: std::io::Error) -> LaboratoryError {
    let detail = error.to_string();
    drop(error);
    failed("laboratory_io", &detail, true)
}

fn json_error(error: serde_json::Error) -> LaboratoryError {
    let detail = error.to_string();
    drop(error);
    failed("laboratory_json", &detail, false)
}

fn process_error(error: peritus_process::ProcessError) -> LaboratoryError {
    let detail = error.to_string();
    drop(error);
    failed("laboratory_process", &detail, false)
}

fn sandbox_error(error: peritus_sandbox::SandboxError) -> LaboratoryError {
    let detail = Box::new(error).to_string();
    failed("laboratory_sandbox", &detail, false)
}

fn linux_error(error: &peritus_sandbox_linux::LinuxError) -> LaboratoryError {
    let detail = error.to_string();
    failed("laboratory_linux", &detail, false)
}
