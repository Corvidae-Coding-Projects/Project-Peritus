//! Reconstruction of retained native backends from current trusted daemon configuration.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use peritus_process::{
    ErrorCode, ExecutionPlan, NativePlatform, ProcessError, ProcessOperation, RecoveryClass,
    RetainedBackendFactoryRequest,
};
use peritus_sandbox::CheckedSandboxPlan;
use peritus_types::Sha256Digest;

use crate::DaemonConfig;

#[cfg(target_os = "linux")]
pub(super) type PlatformRetainedBackend = peritus_sandbox_linux::LinuxBackend;
#[cfg(target_os = "macos")]
pub(super) type PlatformRetainedBackend = peritus_sandbox_macos::MacosBackend;
#[cfg(target_os = "windows")]
pub(super) type PlatformRetainedBackend = peritus_sandbox_windows::WindowsBackend;

/// Rebuilds the exact retained backend from decoded plans and current trusted configuration.
///
/// Backend admission is deliberately restored by the broker only after this function returns a
/// freshly probed descriptor. No target preparation or launch occurs here.
///
/// # Errors
/// Rejects platform, plan, retained identity, current configuration, installation, or probe drift.
#[allow(
    clippy::too_many_arguments,
    reason = "each retained admission axis remains explicit at the service-owner boundary"
)]
pub(super) fn reconstruct_backend(
    request: &RetainedBackendFactoryRequest,
    config: &DaemonConfig,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    expected_descriptor: Sha256Digest,
    expected_support: Sha256Digest,
    expected_preparation: Sha256Digest,
    should_continue: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<PlatformRetainedBackend, ProcessError> {
    validate_outer_binding(
        request,
        execution,
        sandbox,
        expected_descriptor,
        expected_support,
        expected_preparation,
    )?;

    #[cfg(target_os = "linux")]
    {
        reconstruct_linux(request, config, execution, sandbox, should_continue)
    }
    #[cfg(target_os = "macos")]
    {
        reconstruct_macos(request, config, execution, sandbox, should_continue)
    }
    #[cfg(target_os = "windows")]
    {
        reconstruct_windows(request, config, execution, sandbox, should_continue)
    }
}

fn validate_outer_binding(
    request: &RetainedBackendFactoryRequest,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    expected_descriptor: Sha256Digest,
    expected_support: Sha256Digest,
    expected_preparation: Sha256Digest,
) -> Result<(), ProcessError> {
    let backend = execution.backend();
    if request.platform() != NativePlatform::current()
        || execution.sandbox_digest() != sandbox.digest()
        || backend.descriptor_digest() != expected_descriptor
        || backend.support_digest() != expected_support
        || backend.preparation_digest() != expected_preparation
    {
        return Err(mismatch(
            "retained backend request differs from decoded execution authority",
        ));
    }
    Ok(())
}

fn restore_managed_network(
    config: &DaemonConfig,
    sandbox: &CheckedSandboxPlan,
    canonical: &[u8],
    digest: Sha256Digest,
    command_state_root: &Path,
) -> Result<(peritus_product_runner::ManagedGateNetworkGrant, PathBuf), ProcessError> {
    let grants = config
        .managed_gate_network()
        .iter()
        .map(crate::config::ManagedGateNetworkGrantDeclaration::grant)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            unavailable("current managed gate network catalog cannot be reconstructed")
        })?;
    let catalog = peritus_product_runner::ManagedGateNetworkCatalog::new(grants).map_err(|_| {
        unavailable("current managed gate network catalog cannot be reconstructed")
    })?;
    let grant = catalog.resolve_retained(canonical, digest).map_err(|_| {
        mismatch("retained managed gate network grant is no longer authorized")
    })?;
    let cache = grant
        .restore_run_owned_cache(command_state_root, sandbox)
        .map_err(|_| unavailable("retained managed gate cache cannot be restored"))?;
    Ok((grant, cache))
}

#[cfg(target_os = "linux")]
fn reconstruct_linux(
    request: &RetainedBackendFactoryRequest,
    config: &DaemonConfig,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    should_continue: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<PlatformRetainedBackend, ProcessError> {
    use peritus_sandbox_linux::RetainedLinuxBackendFactory;

    let retained = RetainedLinuxBackendFactory::decode(request)?;
    let mut candidates = linux_local_candidates(config, execution)?;
    match retained.managed_network() {
        Some((canonical, digest, command_state_root)) => {
            candidates.push(linux_gate_candidate(
                config,
                execution,
                sandbox,
                Some((canonical, digest, command_state_root)),
            )?);
        }
        None => {
            if let Ok(candidate) = linux_gate_candidate(config, execution, sandbox, None) {
                candidates.push(candidate);
            }
        }
    }
    let selected = select_linux(&retained, candidates)?;
    retained.reconstruct(selected, execution, sandbox, should_continue)
}

#[cfg(target_os = "linux")]
fn select_linux(
    retained: &peritus_sandbox_linux::RetainedLinuxBackendFactory,
    candidates: Vec<peritus_sandbox_linux::LinuxBackendConfig>,
) -> Result<peritus_sandbox_linux::LinuxBackendConfig, ProcessError> {
    for candidate in candidates {
        if retained.matches_config(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(unavailable(
        "no current trusted Linux configuration matches the retained backend",
    ))
}

#[cfg(target_os = "linux")]
fn linux_local_candidates(
    config: &DaemonConfig,
    execution: &ExecutionPlan,
) -> Result<Vec<peritus_sandbox_linux::LinuxBackendConfig>, ProcessError> {
    use peritus_product_runner::LocalCompactorSandbox;

    let Some(local) = config.context().local().local_process.as_ref() else {
        return Ok(Vec::new());
    };
    let LocalCompactorSandbox::Linux { bubblewrap, helper, cgroup_root } = &local.sandbox else {
        return Ok(Vec::new());
    };
    let Some((executable, weights)) = local_compactor_inputs(local, execution)? else {
        return Ok(Vec::new());
    };
    let workspace = execution.working_directory().path().to_path_buf();
    if !valid_local_compactor_workspace(config, &workspace) {
        return Ok(Vec::new());
    }
    let base = peritus_sandbox_linux::LinuxBackendConfig::new(
        workspace.clone(),
        Vec::new(),
        bubblewrap.clone(),
        helper.clone(),
        cgroup_root.clone(),
        None,
    )
    .map_err(|_| unavailable("trusted local Linux backend configuration is invalid"))?
    .with_private_filesystem();
    let mut candidates = vec![base];
    let action = workspace.parent().ok_or_else(|| {
        mismatch("retained local Linux workspace has no authority directory")
    })?;
    let executable_name = executable
        .file_name()
        .ok_or_else(|| mismatch("retained local Linux executable has no file name"))?;
    let weights_name = weights
        .file_name()
        .ok_or_else(|| mismatch("retained local Linux weights have no file name"))?;
    let replacements = vec![
        (
            action.join("input-references").join("executable").join(executable_name),
            executable,
        ),
        (
            action.join("input-references").join("weights").join(weights_name),
            weights,
        ),
    ];
    let bound = peritus_sandbox_linux::LinuxBackendConfig::new(
        workspace,
        Vec::new(),
        bubblewrap.clone(),
        helper.clone(),
        cgroup_root.clone(),
        None,
    )
    .map_err(|_| unavailable("trusted local Linux backend configuration is invalid"))?
    .with_private_filesystem()
    .with_read_only_replacements(replacements);
    if let Ok(bound) = bound {
        candidates.push(bound);
    }
    Ok(candidates)
}

#[cfg(target_os = "linux")]
fn linux_gate_candidate(
    config: &DaemonConfig,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    managed_network: Option<(&[u8], Sha256Digest, &Path)>,
) -> Result<peritus_sandbox_linux::LinuxBackendConfig, ProcessError> {
    let workspace = gate_workspace(execution, sandbox)?;
    let bubblewrap = find_executable("bwrap").ok_or_else(|| {
        unavailable("retained Linux gate requires the installed bubblewrap executable")
    })?;
    let helper = installed_helper("peritus-linux-sandbox-helper")?;
    let cgroup_root = delegated_cgroup_root()?;
    let mut candidate = peritus_sandbox_linux::LinuxBackendConfig::new(
        workspace,
        Vec::new(),
        bubblewrap,
        helper,
        cgroup_root,
        None,
    )
    .map_err(|_| unavailable("trusted Linux gate configuration cannot be reconstructed"))?;
    if let Some((canonical, digest, command_state_root)) = managed_network {
        let (grant, cache) = restore_managed_network(
            config,
            sandbox,
            canonical,
            digest,
            command_state_root,
        )?;
        let proxy = grant.fresh_proxy_preparation().map_err(|_| {
            unavailable("retained Linux managed proxy cannot be reconstructed")
        })?;
        let trusted_canonical = grant.canonical_bytes().to_vec();
        let trusted_digest = grant.digest();
        candidate = candidate
            .with_writable_inputs(vec![cache])
            .and_then(|candidate| {
                candidate
                    .with_managed_proxy(proxy)
                    .with_managed_network_grant(trusted_canonical, trusted_digest)
            })
            .and_then(|candidate| {
                candidate.with_managed_network_cache_root(
                    command_state_root.to_path_buf(),
                    trusted_digest,
                )
            })
            .map_err(|_| {
                unavailable("trusted Linux managed gate configuration cannot be reconstructed")
            })?;
    }
    Ok(candidate)
}

#[cfg(target_os = "macos")]
fn reconstruct_macos(
    request: &RetainedBackendFactoryRequest,
    config: &DaemonConfig,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    should_continue: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<PlatformRetainedBackend, ProcessError> {
    use peritus_sandbox_macos::RetainedMacosBackendFactory;

    let retained = RetainedMacosBackendFactory::decode(request)?;
    let mut candidates = macos_local_candidates(config, execution)?;
    match retained.managed_network() {
        Some((canonical, digest, command_state_root)) => {
            candidates.push(macos_gate_candidate(
                config,
                sandbox,
                Some((canonical, digest, command_state_root)),
            )?);
        }
        None => {
            if let Ok(candidate) = macos_gate_candidate(config, sandbox, None) {
                candidates.push(candidate);
            }
        }
    }
    for candidate in candidates {
        if retained.matches_config(&candidate)? {
            return retained.reconstruct(candidate, execution, sandbox, should_continue);
        }
    }
    Err(unavailable(
        "no current trusted macOS configuration matches the retained backend",
    ))
}

#[cfg(target_os = "macos")]
fn macos_gate_candidate(
    config: &DaemonConfig,
    sandbox: &CheckedSandboxPlan,
    managed_network: Option<(&[u8], Sha256Digest, &Path)>,
) -> Result<peritus_sandbox_macos::PreparationConfig, ProcessError> {
    let helper = installed_helper("peritus-macos-sandbox-helper")?;
    let mut proxy = None;
    let mut retained = None;
    if let Some((canonical, digest, command_state_root)) = managed_network {
        let (grant, _cache) = restore_managed_network(
            config,
            sandbox,
            canonical,
            digest,
            command_state_root,
        )?;
        proxy = Some(grant.fresh_proxy_preparation().map_err(|_| {
            unavailable("retained macOS managed proxy cannot be reconstructed")
        })?);
        retained = Some((
            grant.canonical_bytes().to_vec(),
            grant.digest(),
            command_state_root,
        ));
    }
    let mut candidate = peritus_sandbox_macos::PreparationConfig::new(
        helper,
        PathBuf::from("/usr/bin/sandbox-exec"),
        Vec::new(),
        proxy,
        None,
    )
    .map_err(|_| unavailable("trusted macOS gate configuration cannot be reconstructed"))?;
    if let Some((canonical, digest, command_state_root)) = retained {
        candidate = candidate
            .with_managed_network_grant(canonical, digest)
            .and_then(|candidate| {
                candidate.with_managed_network_cache_root(
                    command_state_root.to_path_buf(),
                    digest,
                )
            })
            .map_err(|_| {
                unavailable("trusted macOS managed gate configuration cannot be reconstructed")
            })?;
    }
    Ok(candidate)
}

#[cfg(target_os = "macos")]
fn macos_local_candidates(
    config: &DaemonConfig,
    execution: &ExecutionPlan,
) -> Result<Vec<peritus_sandbox_macos::PreparationConfig>, ProcessError> {
    use peritus_product_runner::LocalCompactorSandbox;

    let Some(local) = config.context().local().local_process.as_ref() else {
        return Ok(Vec::new());
    };
    let LocalCompactorSandbox::Macos { helper, seatbelt } = &local.sandbox else {
        return Ok(Vec::new());
    };
    if !valid_local_compactor_workspace(config, execution.working_directory().path())
        || local_compactor_inputs(local, execution)?.is_none()
    {
        return Ok(Vec::new());
    }
    let candidate = peritus_sandbox_macos::PreparationConfig::new(
        helper.clone(),
        seatbelt.clone(),
        Vec::new(),
        None,
        None,
    )
    .map_err(|_| unavailable("trusted local macOS backend configuration is invalid"))?;
    Ok(vec![candidate])
}

#[cfg(target_os = "windows")]
fn reconstruct_windows(
    request: &RetainedBackendFactoryRequest,
    config: &DaemonConfig,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    should_continue: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<PlatformRetainedBackend, ProcessError> {
    use peritus_sandbox_windows::RetainedWindowsBackendFactory;

    let retained = RetainedWindowsBackendFactory::decode(request)?;
    let state_root = peritus_sandbox_windows::WindowsPath::from_os_str(
        config.paths().state_root().as_os_str(),
    )
    .map_err(|_| mismatch("daemon state root is not an exact Windows path"))?;
    let acl_backup_root = peritus_sandbox_windows::WindowsPath::from_canonicalized(
        retained.acl_backup_root(),
    )
    .map_err(|_| mismatch("retained ACL backup root is not an exact Windows path"))?;
    if !state_root.contains(&acl_backup_root)
        || retained.acl_backup_root().components().any(|component| {
            matches!(component, std::path::Component::CurDir | std::path::Component::ParentDir)
        })
    {
        return Err(mismatch(
            "retained Windows ACL backup reference is outside daemon-owned state",
        ));
    }
    let mut candidates = windows_local_candidates(config, execution)?;
    match retained.managed_network() {
        Some((canonical, digest, command_state_root)) => {
            candidates.push(windows_gate_candidate(
                config,
                retained.acl_backup_root(),
                execution,
                sandbox,
                Some((canonical, digest, command_state_root)),
            )?);
        }
        None => {
            if let Ok(candidate) = windows_gate_candidate(
                config,
                retained.acl_backup_root(),
                execution,
                sandbox,
                None,
            ) {
                candidates.push(candidate);
            }
        }
    }
    for candidate in candidates {
        if retained.matches_config(&candidate)? {
            return retained.reconstruct(candidate, execution, sandbox, should_continue);
        }
    }
    Err(unavailable(
        "no current trusted Windows configuration matches the retained backend",
    ))
}

#[cfg(target_os = "windows")]
fn windows_local_candidates(
    config: &DaemonConfig,
    execution: &ExecutionPlan,
) -> Result<Vec<peritus_sandbox_windows::WindowsBackendConfig>, ProcessError> {
    use peritus_product_runner::LocalCompactorSandbox;

    let Some(local) = config.context().local().local_process.as_ref() else {
        return Ok(Vec::new());
    };
    let LocalCompactorSandbox::Windows { helper } = &local.sandbox else {
        return Ok(Vec::new());
    };
    let Some((executable, weights)) = local_compactor_inputs(local, execution)? else {
        return Ok(Vec::new());
    };
    let workspace_path = execution.working_directory().path();
    if !valid_local_compactor_workspace(config, workspace_path) {
        return Ok(Vec::new());
    }
    let action = workspace_path.parent().ok_or_else(|| {
        mismatch("retained local Windows workspace has no authority directory")
    })?;
    let name = action
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| mismatch("retained local Windows authority name is invalid"))?;
    let profile = peritus_sandbox_windows::AppContainerProfile::derive_for_current_host(format!(
        "peritus-local-compactor-{name}"
    ))
    .map_err(|_| unavailable("retained local Windows profile cannot be reconstructed"))?;
    let workspace = peritus_sandbox_windows::WindowsPath::from_canonicalized(workspace_path)
        .map_err(|_| mismatch("retained local Windows workspace cannot be normalized"))?;
    let inputs = [executable, weights]
        .into_iter()
        .map(|path| peritus_sandbox_windows::WindowsPath::from_canonicalized(&path))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| mismatch("retained local Windows inputs cannot be normalized"))?;
    let candidate = peritus_sandbox_windows::WindowsBackendConfig::new(
        helper.clone(),
        workspace,
        Vec::new(),
        action.join("acl-backups"),
        peritus_sandbox_windows::TokenProfile::AppContainer(profile),
        None,
        None,
        None,
    )
    .and_then(|value| value.with_read_only_inputs(inputs))
    .map_err(|_| unavailable("trusted local Windows backend configuration is invalid"))?;
    Ok(vec![candidate])
}

#[cfg(target_os = "windows")]
fn windows_gate_candidate(
    config: &DaemonConfig,
    acl_backup_root: &Path,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    managed_network: Option<(&[u8], Sha256Digest, &Path)>,
) -> Result<peritus_sandbox_windows::WindowsBackendConfig, ProcessError> {
    if acl_backup_root.file_name() != Some(OsStr::new("native-gate-acl-backups")) {
        return Err(mismatch(
            "retained Windows gate ACL reference is not the approved gate directory",
        ));
    }
    let helper = installed_helper("peritus-windows-sandbox-helper.exe")?;
    let workspace = peritus_sandbox_windows::WindowsPath::from_canonicalized(
        execution.working_directory().path(),
    )
    .map_err(|_| mismatch("retained Windows gate workspace cannot be normalized"))?;
    let profile = peritus_sandbox_windows::AppContainerProfile::derive_for_current_host(
        "peritus-product-gates",
    )
    .map_err(|_| unavailable("retained Windows gate profile cannot be reconstructed"))?;
    let mut managed_binding = None;
    let (managed_filter, proxy) = match managed_network {
        Some((canonical, digest, command_state_root)) => {
            let (grant, cache) = restore_managed_network(
                config,
                sandbox,
                canonical,
                digest,
                command_state_root,
            )?;
            let proxy = grant.fresh_proxy_preparation().map_err(|_| {
                unavailable("retained Windows managed proxy cannot be reconstructed")
            })?;
            let managed_filter = grant.windows_controller_digest();
            managed_binding = Some((
                cache,
                grant.canonical_bytes().to_vec(),
                grant.digest(),
                command_state_root,
            ));
            (Some(managed_filter), Some(proxy))
        }
        None => (None, None),
    };
    let mut candidate = peritus_sandbox_windows::WindowsBackendConfig::new(
        helper,
        workspace,
        Vec::new(),
        acl_backup_root.to_path_buf(),
        peritus_sandbox_windows::TokenProfile::AppContainer(profile),
        managed_filter,
        proxy,
        None,
    )
    .map_err(|_| unavailable("trusted Windows gate configuration cannot be reconstructed"))?;
    if let Some((cache, canonical, digest, command_state_root)) = managed_binding {
        let cache = peritus_sandbox_windows::WindowsPath::from_canonicalized(&cache)
            .map_err(|_| mismatch("retained Windows managed cache cannot be normalized"))?;
        candidate = candidate
            .with_writable_inputs(vec![cache])
            .and_then(|candidate| {
                candidate.with_managed_network_grant(canonical, digest)
            })
            .and_then(|candidate| {
                candidate.with_managed_network_cache_root(
                    command_state_root.to_path_buf(),
                    digest,
                )
            })
            .map_err(|_| {
                unavailable("trusted Windows managed gate configuration cannot be reconstructed")
            })?;
    }
    Ok(candidate)
}

fn local_compactor_inputs(
    local: &peritus_product_runner::LocalProcessConfig,
    execution: &ExecutionPlan,
) -> Result<Option<(PathBuf, PathBuf)>, ProcessError> {
    let Ok(executable) = local.executable.canonicalize() else {
        return Ok(None);
    };
    let Ok(weights) = local.model_path.canonicalize() else {
        return Ok(None);
    };
    let arguments = execution.command().arguments();
    if execution.command().executable() != executable.as_os_str()
        || arguments.len() != 2
        || arguments[0].as_os_str() != OsStr::new("--model")
        || arguments[1].as_os_str() != weights.as_os_str()
    {
        return Ok(None);
    }
    Ok(Some((executable, weights)))
}

#[cfg(not(target_os = "windows"))]
fn valid_local_compactor_workspace(config: &DaemonConfig, workspace: &Path) -> bool {
    let Some(action) = workspace.parent() else {
        return false;
    };
    workspace.is_absolute()
        && workspace.starts_with(config.paths().state_root())
        && workspace.file_name() == Some(OsStr::new("work"))
        && action.parent().and_then(Path::file_name) == Some(OsStr::new("local-compactor"))
        && !workspace.components().any(|component| {
            matches!(component, std::path::Component::CurDir | std::path::Component::ParentDir)
        })
}

#[cfg(target_os = "windows")]
fn valid_local_compactor_workspace(config: &DaemonConfig, workspace: &Path) -> bool {
    let Some(action) = workspace.parent() else {
        return false;
    };
    let Ok(state_root) = peritus_sandbox_windows::WindowsPath::from_os_str(
        config.paths().state_root().as_os_str(),
    ) else {
        return false;
    };
    let Ok(workspace_path) = peritus_sandbox_windows::WindowsPath::from_canonicalized(workspace)
    else {
        return false;
    };
    workspace.is_absolute()
        && state_root.contains(&workspace_path)
        && workspace.file_name() == Some(OsStr::new("work"))
        && action.parent().and_then(Path::file_name) == Some(OsStr::new("local-compactor"))
        && !workspace.components().any(|component| {
            matches!(component, std::path::Component::CurDir | std::path::Component::ParentDir)
        })
}

#[cfg(target_os = "linux")]
fn gate_workspace(
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
) -> Result<PathBuf, ProcessError> {
    use peritus_sandbox::{FileOperation, PathScope, RuleEffect};

    let rules = sandbox.contract().filesystem().rules();
    for candidate in execution.working_directory().path().ancestors() {
        let projected = project_gate_path(candidate, "workspace")?;
        if rules.iter().any(|rule| {
            rule.effect() == RuleEffect::Allow
                && rule.scope() == PathScope::Descendants
                && rule.path() == &projected
                && rule.operations().contains(FileOperation::Create)
                && rule.operations().contains(FileOperation::Write)
                && rule.operations().contains(FileOperation::Remove)
        }) {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(mismatch(
        "retained Linux gate workspace is absent from exact sandbox authority",
    ))
}

#[cfg(target_os = "linux")]
fn project_gate_path(
    path: &Path,
    domain: &str,
) -> Result<peritus_sandbox::SandboxPath, ProcessError> {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    use std::os::unix::ffi::OsStrExt as _;

    if let Some(text) = path.to_str()
        && let Ok(value) = peritus_sandbox::SandboxPath::new(text.replace('\\', "/"))
    {
        return Ok(value);
    }
    let mut hash = Sha256::new();
    hash.update([1]);
    let bytes = path.as_os_str().as_bytes();
    hash.update(bytes);
    let digest = hash.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}")
            .map_err(|_| mismatch("retained native workspace digest cannot be encoded"))?;
    }
    peritus_sandbox::SandboxPath::new(format!("/__peritus_native/{domain}/{encoded}"))
        .map_err(|_| mismatch("retained native workspace authority cannot be projected"))
}

fn installed_helper(name: &str) -> Result<PathBuf, ProcessError> {
    let executable = std::env::current_exe()
        .map_err(|_| unavailable("retained owner cannot resolve its installation directory"))?;
    let directory = executable.parent().ok_or_else(|| {
        unavailable("retained owner executable has no installation directory")
    })?;
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
        .ok_or_else(|| unavailable("retained owner sandbox helper is unavailable"))
}

#[cfg(target_os = "linux")]
fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "linux")]
fn delegated_cgroup_root() -> Result<PathBuf, ProcessError> {
    let membership = std::fs::read_to_string("/proc/self/cgroup")
        .map_err(|_| unavailable("retained owner cannot read delegated cgroup membership"))?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| unavailable("retained owner requires unified cgroup v2 membership"))?;
    let root = Path::new("/sys/fs/cgroup")
        .canonicalize()
        .map_err(|_| unavailable("retained owner cannot resolve cgroup v2 root"))?;
    let candidate = root.join(relative.trim_start_matches('/'));
    let canonical = candidate
        .canonicalize()
        .map_err(|_| unavailable("retained owner cannot resolve delegated cgroup directory"))?;
    if canonical == root || !canonical.starts_with(&root) {
        return Err(mismatch(
            "retained owner cgroup membership is not an exact delegated subtree",
        ));
    }
    Ok(canonical)
}

const fn mismatch(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::PlanMismatch,
        ProcessOperation::Validate,
        RecoveryClass::Quarantine,
        detail,
    )
}

const fn unavailable(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Unsupported,
        ProcessOperation::Validate,
        RecoveryClass::RetryPreparation,
        detail,
    )
}
