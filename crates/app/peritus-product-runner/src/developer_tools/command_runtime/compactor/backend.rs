//! Select exactly the configured installed native backend; no raw-execution fallback.

use super::detail;
use crate::{LocalCompactorSandbox, LocalProcessConfig};
use peritus_process::NativeSandboxBackend;
use peritus_provider_core::CancellationToken;
use std::path::Path;

#[cfg(target_os = "linux")]
pub(super) type LocalBackend = peritus_sandbox_linux::LinuxBackend;
#[cfg(target_os = "macos")]
pub(super) type LocalBackend = peritus_sandbox_macos::MacosBackend;
#[cfg(target_os = "windows")]
pub(super) type LocalBackend = peritus_sandbox_windows::WindowsBackend;

#[cfg(target_os = "linux")]
pub(super) fn open(
    config: &LocalProcessConfig,
    directory: &Path,
    executable: &Path,
    weights: &Path,
    executable_snapshot: &Path,
    weights_snapshot: &Path,
    bind_snapshots: bool,
    cancellation: &CancellationToken,
) -> Result<LocalBackend, String> {
    let LocalCompactorSandbox::Linux { bubblewrap, helper, cgroup_root } = &config.sandbox else {
        return Err("local compactor sandbox does not match Linux host".to_owned());
    };
    let config = peritus_sandbox_linux::LinuxBackendConfig::new(
        directory.to_path_buf(),
        vec![],
        bubblewrap.clone(),
        helper.clone(),
        cgroup_root.clone(),
        None,
    )
    .map_err(detail)?;
    let config = config.with_private_filesystem();
    let config = if bind_snapshots {
        config
            .with_read_only_replacements(vec![
                (executable_snapshot.to_path_buf(), executable.to_path_buf()),
                (weights_snapshot.to_path_buf(), weights.to_path_buf()),
            ])
            .map_err(detail)?
    } else {
        config
    };
    let cancellation = cancellation.clone();
    peritus_sandbox_linux::LinuxBackend::new_cancellable(config, move || {
        !cancellation.is_cancelled()
    })
    .map_err(detail)
}

#[cfg(target_os = "macos")]
pub(super) fn open(
    config: &LocalProcessConfig,
    _directory: &Path,
    _executable: &Path,
    _weights: &Path,
    _executable_snapshot: &Path,
    _weights_snapshot: &Path,
    _bind_snapshots: bool,
    cancellation: &CancellationToken,
) -> Result<LocalBackend, String> {
    let LocalCompactorSandbox::Macos { helper, seatbelt } = &config.sandbox else {
        return Err("local compactor sandbox does not match macOS host".to_owned());
    };
    let request =
        peritus_sandbox_macos::ProbeRequest::without_proxy(helper.clone(), seatbelt.clone())
            .map_err(detail)?;
    let probe = peritus_sandbox_macos::SystemProbe::run_cancellable(&request, || {
        !cancellation.is_cancelled()
    })
    .map_err(detail)?;
    let config = peritus_sandbox_macos::PreparationConfig::new(
        helper.clone(),
        seatbelt.clone(),
        vec![],
        None,
        None,
    )
    .map_err(detail)?;
    let cancellation = cancellation.clone();
    peritus_sandbox_macos::MacosBackend::new_cancellable(&probe, config, move || {
        !cancellation.is_cancelled()
    })
    .map_err(detail)
}

#[cfg(target_os = "windows")]
pub(super) fn open(
    config: &LocalProcessConfig,
    directory: &Path,
    executable: &Path,
    weights: &Path,
    _executable_snapshot: &Path,
    _weights_snapshot: &Path,
    _bind_snapshots: bool,
    cancellation: &CancellationToken,
) -> Result<LocalBackend, String> {
    let LocalCompactorSandbox::Windows { helper } = &config.sandbox else {
        return Err("local compactor sandbox does not match Windows host".to_owned());
    };
    let name = directory
        .parent()
        .ok_or("missing compactor authority directory")?
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid local compactor identity")?;
    let profile = peritus_sandbox_windows::AppContainerProfile::derive_for_current_host(format!(
        "peritus-local-compactor-{name}"
    ))
    .map_err(detail)?;
    let workspace = peritus_sandbox_windows::WindowsPath::from_canonicalized(directory)
        .map_err(detail)?;
    let inputs = [executable, weights]
        .into_iter()
        .map(|input| {
            peritus_sandbox_windows::WindowsPath::from_canonicalized(input).map_err(detail)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let config = peritus_sandbox_windows::WindowsBackendConfig::new(
        helper.clone(),
        workspace,
        vec![],
        directory.parent().ok_or("missing compactor authority directory")?.join("acl-backups"),
        peritus_sandbox_windows::TokenProfile::AppContainer(profile),
        None,
        None,
        None,
    )
    .map_err(detail)?
    .with_read_only_inputs(inputs)
    .map_err(detail)?;
    let cancellation = cancellation.clone();
    peritus_sandbox_windows::WindowsBackend::new_cancellable(config, move || {
        !cancellation.is_cancelled()
    })
    .map_err(detail)
}
