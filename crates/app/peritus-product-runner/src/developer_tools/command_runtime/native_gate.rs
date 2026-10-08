//! Restricted native sandbox preparation for exact-target gate commands.

use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use peritus_process::{
    CommandSpec, EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, EnvironmentVariable,
    IoMode, NativeSandboxBackend, ProcessResourcePolicy, StdinPolicy, WorkspaceAccess,
};
use peritus_provider_core::CancellationToken;
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, CheckedSandboxPlan, DescendantPolicy,
    EnvironmentContract, EnvironmentMode, EnvironmentName, EnvironmentRequirements,
    FileOperation, FileOperationSet, FileRequirement, FilesystemContract, FilesystemRule,
    InputPermission, IsolationRequirement, NativeExecutionAuthority, NetworkContract, PathScope,
    ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TreeContainment, admit_backend,
    compile_sandbox,
};
use peritus_types::{ResourceQuantity, Sha256Digest};
use sha2::{Digest as _, Sha256};

use super::{ManagedGateNetworkGrant, identity::CommandIds};

const TERMINAL_EVENT_RECORDS: u64 = 16_384;

/// Exact retained invocation requested by the candidate gate owner.
pub(crate) struct GateInvocationRequest {
    pub(super) operation_key: Sha256Digest,
    pub(super) program: String,
    pub(super) arguments: Vec<String>,
    pub(super) cwd: PathBuf,
    pub(super) authority_live: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) cancellation: CancellationToken,
    pub(super) network: Option<ManagedGateNetworkGrant>,
}

impl GateInvocationRequest {
    pub(crate) fn new(
        operation_key: Sha256Digest,
        program: String,
        arguments: Vec<String>,
        cwd: PathBuf,
        authority_live: Arc<dyn Fn() -> bool + Send + Sync>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            operation_key,
            program,
            arguments,
            cwd,
            authority_live,
            cancellation,
            network: None,
        }
    }

    pub(crate) fn new_managed(
        operation_key: Sha256Digest,
        program: String,
        arguments: Vec<String>,
        cwd: PathBuf,
        authority_live: Arc<dyn Fn() -> bool + Send + Sync>,
        cancellation: CancellationToken,
        network: ManagedGateNetworkGrant,
    ) -> Self {
        Self {
            operation_key,
            program,
            arguments,
            cwd,
            authority_live,
            cancellation,
            network: Some(network),
        }
    }

    pub(super) fn request_digest(&self) -> Sha256Digest {
        let legacy = self.legacy_request_digest();
        let Some(network) = &self.network else { return legacy };
        let mut hasher = Sha256::new();
        hasher.update(b"peritus-native-gate-invocation-v3\0");
        hasher.update(legacy.as_bytes());
        hasher.update(network.digest().as_bytes());
        Sha256Digest::new(hasher.finalize().into())
    }

    fn legacy_request_digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus-native-gate-invocation-v2\0");
        hasher.update(self.operation_key.as_bytes());
        hash_bytes(&mut hasher, self.program.as_bytes());
        hasher.update(u64::try_from(self.arguments.len()).unwrap_or(u64::MAX).to_le_bytes());
        for argument in &self.arguments {
            hash_bytes(&mut hasher, argument.as_bytes());
        }
        hash_native_os_str(&mut hasher, self.cwd.as_os_str());
        Sha256Digest::new(hasher.finalize().into())
    }
}

/// Retained outcome of one exact native gate operation.
pub(crate) enum GateInvocationOutcome {
    Complete { exit_code: Option<i32>, preview: String },
    Pending { detail: String },
    Unavailable { detail: String },
    AuthorityRevoked { detail: String },
}

#[cfg(target_os = "linux")]
pub(super) type GateBackend = peritus_sandbox_linux::LinuxBackend;
#[cfg(target_os = "macos")]
pub(super) type GateBackend = peritus_sandbox_macos::MacosBackend;
#[cfg(target_os = "windows")]
pub(super) type GateBackend = peritus_sandbox_windows::WindowsBackend;

/// One native backend selected from current installed resources and admitted to an exact plan.
pub(super) struct PreparedNativeGate {
    pub(super) checked: CheckedSandboxPlan,
    pub(super) admission: BackendAdmission,
    pub(super) backend: GateBackend,
}

/// Resolves one gate executable to the same exact native path used by authorization and launch.
pub(super) fn resolve_executable(program: &str, cwd: &Path) -> Result<PathBuf, String> {
    let requested = Path::new(program);
    let candidates = if requested.is_absolute() {
        vec![requested.to_path_buf()]
    } else if requested.components().count() > 1 {
        vec![cwd.join(requested)]
    } else {
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .flat_map(|directory| {
                executable_extensions(program).into_iter().map(move |extension| {
                    let mut name = OsString::from(program);
                    name.push(extension);
                    directory.join(name)
                })
            })
            .collect()
    };
    for candidate in candidates {
        let candidate = if candidate.is_absolute() { candidate } else { cwd.join(candidate) };
        if candidate.is_file()
            && let Some(parent) = candidate.parent()
            && let Ok(parent) = parent.canonicalize()
            && let Some(name) = candidate.file_name()
        {
            return Ok(parent.join(name));
        }
    }
    Err(format!("restricted gate executable `{program}` is unavailable"))
}

/// Snapshots the non-secret host variables needed by local language toolchains.
pub(super) fn environment(
    program: &str,
    managed_cache: Option<&Path>,
) -> Result<EnvironmentPlan, String> {
    environment_with_bindings(program, managed_cache, Vec::new())
}

pub(super) fn observational_environment(
    program: &str,
    bindings: Vec<(String, String)>,
) -> Result<EnvironmentPlan, String> {
    environment_with_bindings(program, None, bindings)
}

fn environment_with_bindings(
    program: &str,
    managed_cache: Option<&Path>,
    additional_bindings: Vec<(String, String)>,
) -> Result<EnvironmentPlan, String> {
    let mut allowlist = vec![
        "CARGO_BUILD_JOBS",
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
        "CMAKE_BUILD_PARALLEL_LEVEL",
        "GOCACHE",
        "GOMAXPROCS",
        "GOMODCACHE",
        "GOPATH",
        "GOROOT",
        "LANG",
        "LC_ALL",
        "MAKEFLAGS",
        "MAX_JOBS",
        "NODE_PATH",
        "NUM_JOBS",
        "PATH",
        "PATHEXT",
        "PERITUS_RECOMMENDED_PARALLELISM",
        "PYTHONPATH",
        "RAYON_NUM_THREADS",
        "RUSTFLAGS",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "SystemRoot",
        "TEMP",
        "TMP",
        "TMPDIR",
        "VIRTUAL_ENV",
        "WINDIR",
        "npm_config_cache",
        "npm_config_jobs",
    ];
    if managed_cache.is_some() {
        allowlist.retain(|name| {
            !matches!(
                *name,
                "CARGO_HOME"
                    | "CARGO_TARGET_DIR"
                    | "GOCACHE"
                    | "GOMODCACHE"
                    | "GOPATH"
                    | "npm_config_cache"
            )
        });
    }
    let mut bindings = BTreeMap::new();
    if program == "go" && managed_cache.is_some() {
        for (name, value) in [
            ("GOPROXY", "https://proxy.golang.org"),
            ("GOSUMDB", "sum.golang.org"),
        ] {
            bindings.insert(name.to_owned(), value.to_owned());
        }
    }
    if let Some(cache) = managed_cache {
        for (name, path) in [
            ("CARGO_HOME", cache.join("cargo-home")),
            ("CARGO_TARGET_DIR", cache.join("cargo-target")),
            ("GOCACHE", cache.join("go-build")),
            ("GOMODCACHE", cache.join("go-mod")),
            ("GOPATH", cache.join("go-path")),
            ("npm_config_cache", cache.join("npm")),
        ] {
            bindings.insert(name.to_owned(), path.to_string_lossy().into_owned());
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        for (name, path) in [
            ("CARGO_HOME", PathBuf::from(&home).join(".cargo")),
            ("RUSTUP_HOME", PathBuf::from(home).join(".rustup")),
        ] {
            if (name != "CARGO_HOME" || managed_cache.is_none())
                && std::env::var_os(name).is_none()
                && path.is_dir()
            {
                bindings.insert(name.to_owned(), path.to_string_lossy().into_owned());
            }
        }
    }
    for (name, value) in additional_bindings {
        bindings.insert(name, value);
    }
    let bindings = bindings
        .into_iter()
        .map(|(name, value)| {
            EnvironmentVariable::new(name, value)
                .map_err(|error| format!("construct restricted command environment: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    EnvironmentPlan::allowlisted(allowlist, bindings)
        .map_err(|error| format!("snapshot gate environment: {error}"))
}

/// Compiles and admits a restricted gate against the current platform's real native backend.
///
/// Missing helpers, unsupported kernels, or unavailable delegated containment fail before process
/// authority is consumed. The product lifecycle can therefore retain the exact candidate and ask
/// for the missing host capability without falling back to ambient execution.
#[allow(
    clippy::too_many_arguments,
    reason = "the native gate boundary binds every authority-relevant process input"
)]
pub(super) fn prepare(
    ids: &CommandIds,
    command: &CommandSpec,
    workspace_root: &Path,
    working_directory: &Path,
    environment: &EnvironmentPlan,
    resources: ProcessResourcePolicy,
    state_root: &Path,
    managed_network: Option<&ManagedGateNetworkGrant>,
    managed_cache: Option<&Path>,
    cancellation: &CancellationToken,
) -> Result<PreparedNativeGate, String> {
    ensure_not_cancelled(cancellation)?;
    if managed_network.is_some() != managed_cache.is_some() {
        return Err("managed gate network and run-owned cache must be selected together".to_owned());
    }
    let checked = checked_plan(
        ids,
        command,
        workspace_root,
        working_directory,
        environment,
        resources,
        IoMode::Pipes,
        StdinPolicy::Closed,
        WorkspaceAccess::Writable,
        &[],
        managed_network,
        managed_cache,
    )?;
    ensure_not_cancelled(cancellation)?;
    let backend = open_backend(
        workspace_root,
        working_directory,
        state_root,
        managed_network,
        managed_cache,
        cancellation,
    )?;
    ensure_not_cancelled(cancellation)?;
    let admission = admit_backend(&checked, backend.descriptor(), AdmissionProfile::Production)
        .map_err(|error| format!("admit restricted native gate backend: {error}"))?;
    Ok(PreparedNativeGate { checked, admission, backend })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the observational command boundary binds every authority-relevant process input"
)]
pub(super) fn prepare_observational(
    ids: &CommandIds,
    command: &CommandSpec,
    workspace_root: &Path,
    working_directory: &Path,
    environment: &EnvironmentPlan,
    io: IoMode,
    stdin: StdinPolicy,
    resources: ProcessResourcePolicy,
    state_root: &Path,
    cancellation: &CancellationToken,
) -> Result<PreparedNativeGate, String> {
    ensure_not_cancelled(cancellation)?;
    let checked = checked_plan(
        ids,
        command,
        workspace_root,
        working_directory,
        environment,
        resources,
        io,
        stdin,
        WorkspaceAccess::ReadOnly,
        &[],
        None,
        None,
    )?;
    ensure_not_cancelled(cancellation)?;
    let backend = open_backend(
        workspace_root,
        workspace_root,
        state_root,
        None,
        None,
        cancellation,
    )
    .map_err(|error| format!("open enforced read-only command backend: {error}"))?;
    ensure_not_cancelled(cancellation)?;
    let admission = admit_backend(&checked, backend.descriptor(), AdmissionProfile::Production)
        .map_err(|error| format!("admit enforced read-only command backend: {error}"))?;
    Ok(PreparedNativeGate { checked, admission, backend })
}

#[allow(
    clippy::too_many_arguments,
    reason = "protected-path command confinement binds every authority-relevant process input"
)]
pub(super) fn prepare_confined_mutation(
    ids: &CommandIds,
    command: &CommandSpec,
    workspace_root: &Path,
    working_directory: &Path,
    environment: &EnvironmentPlan,
    io: IoMode,
    stdin: StdinPolicy,
    resources: ProcessResourcePolicy,
    protected_paths: &[PathBuf],
    state_root: &Path,
    cancellation: &CancellationToken,
) -> Result<PreparedNativeGate, String> {
    ensure_not_cancelled(cancellation)?;
    let checked = checked_plan(
        ids,
        command,
        workspace_root,
        working_directory,
        environment,
        resources,
        io,
        stdin,
        WorkspaceAccess::Writable,
        protected_paths,
        None,
        None,
    )
    .map_err(|error| {
        format!(
            "protected-path command confinement is unavailable: {error}; dismiss or rebind the exact leave-alone constraint, or use workspace file tools outside that path"
        )
    })?;
    ensure_not_cancelled(cancellation)?;
    let backend = open_backend(
        workspace_root,
        workspace_root,
        state_root,
        None,
        None,
        cancellation,
    )
    .map_err(|error| {
        format!(
            "protected-path command confinement is unavailable: {error}; install the reported sandbox capability, dismiss or rebind the exact leave-alone constraint, or use workspace file tools outside that path"
        )
    })?;
    ensure_not_cancelled(cancellation)?;
    let admission = admit_backend(&checked, backend.descriptor(), AdmissionProfile::Production)
        .map_err(|error| {
            format!(
                "protected-path command confinement is unavailable: {error}; select a backend that can enforce every protected path before retrying this command"
            )
        })?;
    Ok(PreparedNativeGate { checked, admission, backend })
}

fn checked_plan(
    ids: &CommandIds,
    command: &CommandSpec,
    workspace_root: &Path,
    working_directory: &Path,
    environment: &EnvironmentPlan,
    resources: ProcessResourcePolicy,
    io: IoMode,
    stdin: StdinPolicy,
    workspace_access: WorkspaceAccess,
    protected_paths: &[PathBuf],
    managed_network: Option<&ManagedGateNetworkGrant>,
    managed_cache: Option<&Path>,
) -> Result<CheckedSandboxPlan, String> {
    let executable = native_path(command.executable(), "program")?;
    let workspace = native_path(workspace_root.as_os_str(), "workspace")?;
    let names = environment_names(environment)?;
    let inherited = native_environment_shadows(&names.inherited)?;
    let literals = native_environment_shadows(&names.literals)?;
    let mode = match environment.source() {
        EnvironmentSource::Cleared => EnvironmentMode::Cleared,
        EnvironmentSource::Allowlisted(_) => EnvironmentMode::AllowListed(inherited.clone()),
    };
    let environment_contract = EnvironmentContract::new(mode, literals.clone())
        .map_err(|error| format!("construct gate environment contract: {error}"))?;
    let limits = resource_limits(resources)?;
    let terminal_events = ResourceQuantity::new(TERMINAL_EVENT_RECORDS);
    let terminal_size = match io {
        IoMode::Pipes => None,
        IoMode::Pty(size) => Some(
            peritus_sandbox::TerminalSize::new(size.columns(), size.rows())
                .map_err(|error| format!("construct gate terminal size: {error}"))?,
        ),
    };
    let input = if matches!(stdin, StdinPolicy::Closed) {
        InputPermission::Denied
    } else {
        InputPermission::Allowed
    };
    let resize = if matches!(io, IoMode::Pty(_)) {
        ResizePermission::Allowed
    } else {
        ResizePermission::Denied
    };
    let terminal_mode = if matches!(io, IoMode::Pty(_)) {
        TerminalMode::Pty
    } else {
        TerminalMode::Pipes
    };
    let terminal_limits =
        TerminalLimits::with_optional_output(terminal_size, terminal_events, None)
        .map_err(|error| format!("construct gate terminal limits: {error}"))?;
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([terminal_mode]),
        input,
        resize,
        TerminalSignalPermission::Allowed,
        terminal_limits,
    )
    .map_err(|error| format!("construct gate terminal contract: {error}"))?;
    let terminal_requirements = TerminalRequirements::with_optional_output(
        terminal_mode,
        input,
        resize,
        TerminalSignalPermission::Allowed,
        terminal_size,
        terminal_events,
        None,
    )
    .map_err(|error| format!("construct gate terminal requirements: {error}"))?;

    let mut workspace_operations = vec![
        FileOperation::Discover,
        FileOperation::Metadata,
        FileOperation::Read,
        FileOperation::Execute,
    ];
    if workspace_access == WorkspaceAccess::Writable {
        workspace_operations.extend([
            FileOperation::Create,
            FileOperation::Write,
            FileOperation::Remove,
        ]);
    }
    let mut rules = vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            executable.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Read,
                FileOperation::Execute,
            ]),
        )
        .map_err(|error| format!("construct gate executable rule: {error}"))?,
        FilesystemRule::new(
            RuleEffect::Allow,
            workspace.clone(),
            PathScope::Descendants,
            FileOperationSet::from_operations(workspace_operations),
        )
        .map_err(|error| format!("construct gate workspace rule: {error}"))?,
    ];
    if let Some(cache) = managed_cache {
        rules.push(
            FilesystemRule::new(
                RuleEffect::Allow,
                native_path(cache.as_os_str(), "managed-cache")?,
                PathScope::Descendants,
                FileOperationSet::from_operations([
                    FileOperation::Discover,
                    FileOperation::Metadata,
                    FileOperation::Read,
                    FileOperation::Execute,
                    FileOperation::Create,
                    FileOperation::Write,
                    FileOperation::Remove,
                ]),
            )
            .map_err(|error| format!("construct managed gate cache rule: {error}"))?,
        );
    }
    for root in runtime_roots(command.executable(), environment, workspace_root, managed_cache) {
        let root = native_path(root.as_os_str(), "runtime")?;
        rules.push(
            FilesystemRule::new(
                RuleEffect::Allow,
                root,
                PathScope::Descendants,
                FileOperationSet::from_operations([
                    FileOperation::Discover,
                    FileOperation::Metadata,
                    FileOperation::Read,
                    FileOperation::Execute,
                ]),
            )
            .map_err(|error| format!("construct gate runtime rule: {error}"))?,
        );
    }
    for relative in protected_paths {
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "protected path {} is not workspace-relative",
                relative.display(),
            ));
        }
        rules.push(
            FilesystemRule::new(
                RuleEffect::Deny,
                native_path(workspace_root.join(relative).as_os_str(), "protected-path")?,
                PathScope::Descendants,
                FileOperationSet::from_operations([
                    FileOperation::Discover,
                    FileOperation::Metadata,
                    FileOperation::Read,
                    FileOperation::Execute,
                    FileOperation::Create,
                    FileOperation::Write,
                    FileOperation::Remove,
                ]),
            )
            .map_err(|error| format!("construct protected-path confinement rule: {error}"))?,
        );
    }
    let filesystem = FilesystemContract::new(rules)
        .map_err(|error| format!("construct gate filesystem contract: {error}"))?;
    let process = ProcessContract::with_optional_process_limit(
        vec![executable.clone()],
        DescendantPolicy::Allowed,
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        None,
    )
    .map_err(|error| format!("construct gate process contract: {error}"))?;
    let network = managed_network
        .map_or_else(|| Ok(NetworkContract::deny_all()), ManagedGateNetworkGrant::network_contract)
        .map_err(|error| format!("construct managed gate network contract: {error}"))?;
    let contract = SandboxContract::new(
        filesystem,
        process,
        environment_contract,
        network,
        SecretContract::deny_all(),
        limits,
        terminal,
    );
    let mut files = vec![
            FileRequirement::new(executable.clone(), FileOperation::Execute),
            FileRequirement::new(workspace, FileOperation::Read),
        ];
    if let Some(cache) = managed_cache {
        files.push(FileRequirement::new(
            native_path(cache.as_os_str(), "managed-cache")?,
            FileOperation::Write,
        ));
    }
    let network_requirements = managed_network
        .map_or_else(|| Ok(Vec::new()), ManagedGateNetworkGrant::network_requirements)
        .map_err(|error| format!("construct managed gate network requirements: {error}"))?;
    let requirements = SandboxRequirements::new(
        files,
        ProcessRequirements::new(executable, 0, true),
        EnvironmentRequirements::new(inherited, literals)
            .map_err(|error| format!("construct gate environment requirements: {error}"))?,
        network_requirements,
        Vec::new(),
        limits,
        terminal_requirements,
    )
    .map_err(|error| format!("construct gate sandbox requirements: {error}"))?;
    compile_sandbox(
        SandboxBinding::new(ids.process, ids.resource, ids.environment, ids.revision),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .and_then(|checked| {
        checked.bind_native_execution(NativeExecutionAuthority::new(
            command.executable().to_os_string(),
            working_directory.to_path_buf(),
            names.inherited,
            names.literals,
        )?)
    })
    .map_err(|error| format!("compile restricted native gate sandbox: {error}"))
}

fn runtime_roots(
    executable: &OsStr,
    environment: &EnvironmentPlan,
    workspace_root: &Path,
    managed_cache: Option<&Path>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(parent) = Path::new(executable).parent() {
        roots.push(parent.to_path_buf());
    }
    for variable in environment.variables() {
        let name = variable.name().to_string_lossy();
        if name.eq_ignore_ascii_case("PATH") {
            roots.extend(
                std::env::split_paths(variable.value()).filter(|path| path.is_absolute()),
            );
        } else if name.eq_ignore_ascii_case("RUSTUP_HOME")
            || name.eq_ignore_ascii_case("GOROOT")
        {
            let path = PathBuf::from(variable.value());
            if path.is_absolute() {
                roots.push(path);
            }
        } else if name.eq_ignore_ascii_case("CARGO_HOME") {
            let path = PathBuf::from(variable.value());
            if path.is_absolute() {
                roots.extend([path.join("bin"), path.join("git"), path.join("registry")]);
            }
        }
    }
    #[cfg(unix)]
    roots.extend(
        ["/bin", "/lib", "/lib64", "/usr", "/opt"]
            .into_iter()
            .map(PathBuf::from),
    );
    #[cfg(target_os = "macos")]
    roots.push(PathBuf::from("/System"));
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        roots.push(PathBuf::from(system_root));
    }
    roots.retain(|path| {
        path.is_absolute()
            && path.exists()
            && !path.starts_with(workspace_root)
            && managed_cache.is_none_or(|cache| !path.starts_with(cache))
    });
    roots.sort();
    roots.dedup();
    roots
}

fn environment_names(environment: &EnvironmentPlan) -> Result<EnvironmentNames, String> {
    let inherited = match environment.source() {
        EnvironmentSource::Cleared => Vec::new(),
        EnvironmentSource::Allowlisted(names) => names.clone(),
    };
    let literals = environment
        .variables()
        .iter()
        .filter(|variable| variable.source() == EnvironmentValueSource::Literal)
        .map(|variable| variable.name().to_os_string())
        .collect();
    Ok(EnvironmentNames { inherited, literals })
}

struct EnvironmentNames {
    inherited: Vec<OsString>,
    literals: Vec<OsString>,
}

fn native_environment_shadows(names: &[OsString]) -> Result<Vec<EnvironmentName>, String> {
    names
        .iter()
        .map(|name| {
            EnvironmentName::new(format!("_PERITUS_NATIVE_{}", native_digest(name)))
                .map_err(|error| format!("construct native gate environment authority: {error}"))
        })
        .collect()
}

fn resource_limits(resources: ProcessResourcePolicy) -> Result<ResourceLimits, String> {
    ResourceLimits::with_optional_limits(
        resources.wall_millis().map(ResourceQuantity::new),
        resources.cpu_millis().map(ResourceQuantity::new),
        resources.memory_limit().map(ResourceQuantity::new),
        resources.disk_limit().map(ResourceQuantity::new),
        resources.output_limit().map(ResourceQuantity::new),
        resources.file_descriptor_limit().map(ResourceQuantity::new),
        resources.process_limit().map(ResourceQuantity::new),
        Some(ResourceQuantity::new(resources.concurrent_slots())),
    )
    .map_err(|error| format!("construct gate resource contract: {error}"))
}

fn native_path(value: &OsStr, domain: &str) -> Result<SandboxPath, String> {
    if let Some(text) = value.to_str() {
        let normalized = text.replace('\\', "/");
        if let Ok(path) = SandboxPath::new(normalized) {
            return Ok(path);
        }
    }
    SandboxPath::new(format!("/__peritus_native/{domain}/{}", native_digest(value)))
        .map_err(|error| format!("construct native gate {domain} path: {error}"))
}

fn native_digest(value: &OsStr) -> String {
    let mut bytes = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        bytes.push(1);
        bytes.extend_from_slice(value.as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        bytes.push(2);
        for unit in value.encode_wide() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
    }
    let digest = peritus_codec::sha256(&bytes);
    let mut encoded = String::with_capacity(digest.as_bytes().len() * 2);
    for byte in digest.as_bytes() {
        write!(&mut encoded, "{byte:02x}").expect("writing hexadecimal into String cannot fail");
    }
    encoded
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

fn hash_native_os_str(hasher: &mut Sha256, value: &OsStr) {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;

        hasher.update([1]);
        hash_bytes(hasher, value.as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;

        hasher.update([2]);
        let units = value.encode_wide().collect::<Vec<_>>();
        hasher.update(u64::try_from(units.len()).unwrap_or(u64::MAX).to_le_bytes());
        for unit in units {
            hasher.update(unit.to_be_bytes());
        }
    }
}

#[cfg(target_os = "linux")]
fn open_backend(
    workspace_root: &Path,
    _working_directory: &Path,
    state_root: &Path,
    managed_network: Option<&ManagedGateNetworkGrant>,
    managed_cache: Option<&Path>,
    cancellation: &CancellationToken,
) -> Result<GateBackend, String> {
    let bubblewrap = find_executable("bwrap")
        .ok_or_else(|| "restricted gates require an installed bubblewrap executable".to_owned())?;
    let helper = installed_helper("peritus-linux-sandbox-helper")?;
    let cgroup_root = delegated_cgroup_root()?;
    let mut config = peritus_sandbox_linux::LinuxBackendConfig::new(
        workspace_root.to_path_buf(),
        Vec::new(),
        bubblewrap,
        helper,
        cgroup_root,
        None,
    )
    .map_err(|error| format!("construct restricted gate Linux backend: {error}"))?;
    if let Some(cache) = managed_cache {
        config = config
            .with_writable_inputs(vec![cache.to_path_buf()])
            .map_err(|error| format!("construct managed gate Linux cache: {error}"))?;
    }
    if let Some(grant) = managed_network {
        let proxy = grant
            .fresh_proxy_preparation()
            .map_err(|error| format!("construct managed gate Linux proxy: {error}"))?;
        config = config
            .with_managed_proxy(proxy)
            .with_managed_network_grant(grant.canonical_bytes().to_vec(), grant.digest())
            .and_then(|config| {
                config.with_managed_network_cache_root(state_root.to_path_buf(), grant.digest())
            })
            .map_err(|error| format!("bind managed gate Linux grant: {error}"))?;
    }
    let cancellation = cancellation.clone();
    peritus_sandbox_linux::LinuxBackend::new_cancellable(config, move || {
        !cancellation.is_cancelled()
    })
    .map_err(|error| format!("open restricted gate Linux backend: {error}"))
}

#[cfg(target_os = "macos")]
fn open_backend(
    _workspace_root: &Path,
    _working_directory: &Path,
    state_root: &Path,
    managed_network: Option<&ManagedGateNetworkGrant>,
    _managed_cache: Option<&Path>,
    cancellation: &CancellationToken,
) -> Result<GateBackend, String> {
    let helper = installed_helper("peritus-macos-sandbox-helper")?;
    let seatbelt = PathBuf::from("/usr/bin/sandbox-exec");
    let request = peritus_sandbox_macos::ProbeRequest::without_proxy(
        helper.clone(),
        seatbelt.clone(),
    )
    .map_err(|error| format!("construct restricted gate macOS probe: {error}"))?;
    let probe = peritus_sandbox_macos::SystemProbe::run_cancellable(&request, || {
        !cancellation.is_cancelled()
    })
    .map_err(|error| format!("probe restricted gate macOS backend: {error}"))?;
    let mut config = peritus_sandbox_macos::PreparationConfig::new(
        helper,
        seatbelt,
        Vec::new(),
        managed_network
            .map(ManagedGateNetworkGrant::fresh_proxy_preparation)
            .transpose()
            .map_err(|error| format!("construct managed gate macOS proxy: {error}"))?,
        None,
    )
    .map_err(|error| format!("construct restricted gate macOS backend: {error}"))?;
    if let Some(grant) = managed_network {
        config = config
            .with_managed_network_grant(grant.canonical_bytes().to_vec(), grant.digest())
            .and_then(|config| {
                config.with_managed_network_cache_root(state_root.to_path_buf(), grant.digest())
            })
            .map_err(|error| format!("bind managed gate macOS grant: {error}"))?;
    }
    let cancellation = cancellation.clone();
    peritus_sandbox_macos::MacosBackend::new_cancellable(&probe, config, move || {
        !cancellation.is_cancelled()
    })
        .map_err(|error| format!("open restricted gate macOS backend: {error}"))
}

#[cfg(target_os = "windows")]
fn open_backend(
    _workspace_root: &Path,
    working_directory: &Path,
    state_root: &Path,
    managed_network: Option<&ManagedGateNetworkGrant>,
    managed_cache: Option<&Path>,
    cancellation: &CancellationToken,
) -> Result<GateBackend, String> {
    let helper = installed_helper("peritus-windows-sandbox-helper.exe")?;
    let workspace = peritus_sandbox_windows::WindowsPath::from_canonicalized(working_directory)
        .map_err(|error| format!("construct restricted gate Windows workspace: {error}"))?;
    let profile = peritus_sandbox_windows::AppContainerProfile::derive_for_current_host(
        "peritus-product-gates",
    )
    .map_err(|error| format!("construct restricted gate AppContainer: {error}"))?;
    let proxy = managed_network
        .map(ManagedGateNetworkGrant::fresh_proxy_preparation)
        .transpose()
        .map_err(|error| format!("construct managed gate Windows proxy: {error}"))?;
    let mut config = peritus_sandbox_windows::WindowsBackendConfig::new(
        helper,
        workspace,
        Vec::new(),
        state_root.join("native-gate-acl-backups"),
        peritus_sandbox_windows::TokenProfile::AppContainer(profile),
        managed_network.map(ManagedGateNetworkGrant::windows_controller_digest),
        proxy,
        None,
    )
    .map_err(|error| format!("construct restricted gate Windows backend: {error}"))?;
    if let Some(cache) = managed_cache {
        let cache = peritus_sandbox_windows::WindowsPath::from_canonicalized(cache)
            .map_err(|error| format!("construct managed gate Windows cache: {error}"))?;
        config = config
            .with_writable_inputs(vec![cache])
            .map_err(|error| format!("admit managed gate Windows cache: {error}"))?;
    }
    if let Some(grant) = managed_network {
        config = config
            .with_managed_network_grant(grant.canonical_bytes().to_vec(), grant.digest())
            .and_then(|config| {
                config.with_managed_network_cache_root(state_root.to_path_buf(), grant.digest())
            })
            .map_err(|error| format!("bind managed gate Windows grant: {error}"))?;
    }
    let cancellation = cancellation.clone();
    peritus_sandbox_windows::WindowsBackend::new_cancellable(config, move || {
        !cancellation.is_cancelled()
    })
    .map_err(|error| format!("open restricted gate Windows backend: {error}"))
}

fn installed_helper(name: &str) -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|_| "resolve current executable for restricted gate helper".to_owned())?;
    let directory = executable
        .parent()
        .ok_or_else(|| "current executable has no installation directory".to_owned())?;
    let mut candidates = vec![directory.join(name)];
    if let Some(parent) = directory.parent() {
        candidates.push(parent.join("libexec").join(name));
    }
    #[cfg(unix)]
    candidates.extend([
        PathBuf::from("/usr/libexec/peritus").join(name),
        PathBuf::from("/usr/lib/peritus").join(name),
    ]);
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("restricted gates require installed helper `{name}`"))
}

fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn executable_extensions(program: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};

        if Path::new(program).extension().is_some() {
            return vec![OsString::new()];
        }
        let value = std::env::var_os("PATHEXT")
            .unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
        let units = value.encode_wide().collect::<Vec<_>>();
        let mut extensions = units
            .split(|unit| *unit == u16::from(b';'))
            .filter(|extension| !extension.is_empty())
            .map(OsString::from_wide)
            .collect::<Vec<_>>();
        extensions.push(OsString::new());
        extensions
    }
    #[cfg(not(windows))]
    {
        let _ = program;
        vec![OsString::new()]
    }
}

#[cfg(target_os = "linux")]
fn delegated_cgroup_root() -> Result<PathBuf, String> {
    let membership = std::fs::read_to_string("/proc/self/cgroup")
        .map_err(|_| "read current delegated cgroup membership".to_owned())?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| "restricted gates require a unified cgroup v2 membership".to_owned())?;
    let root = Path::new("/sys/fs/cgroup")
        .canonicalize()
        .map_err(|_| "resolve cgroup v2 filesystem root".to_owned())?;
    let candidate = root.join(relative.trim_start_matches('/'));
    let canonical = candidate
        .canonicalize()
        .map_err(|_| "resolve current delegated cgroup directory".to_owned())?;
    if canonical == root || !canonical.starts_with(&root) {
        return Err("restricted gates require an exact delegated cgroup subtree".to_owned());
    }
    Ok(canonical)
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), String> {
    if cancellation.is_cancelled() {
        Err("restricted gate preparation cancelled".to_owned())
    } else {
        Ok(())
    }
}
