//! Select exactly the configured installed native backend; no raw-execution fallback.

use super::detail;
use crate::{LocalCompactorSandbox, LocalProcessConfig};
use peritus_process::NativeSandboxBackend;
use std::path::Path;

#[cfg(target_os = "linux")]
pub(super) fn open(
    config: &LocalProcessConfig,
    directory: &Path,
) -> Result<impl NativeSandboxBackend, String> {
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
    peritus_sandbox_linux::LinuxBackend::new(config.with_private_filesystem()).map_err(detail)
}

#[cfg(target_os = "macos")]
pub(super) fn open(
    config: &LocalProcessConfig,
    _directory: &Path,
) -> Result<impl NativeSandboxBackend, String> {
    let LocalCompactorSandbox::Macos { helper, seatbelt } = &config.sandbox else {
        return Err("local compactor sandbox does not match macOS host".to_owned());
    };
    let request = peritus_sandbox_macos::ProbeRequest::new(
        helper.clone(),
        seatbelt.clone(),
        None,
        std::time::Duration::from_millis(250),
    )
    .map_err(detail)?;
    let probe = peritus_sandbox_macos::SystemProbe::run(&request).map_err(detail)?;
    let config = peritus_sandbox_macos::PreparationConfig::new(
        helper.clone(),
        seatbelt.clone(),
        vec![],
        None,
        None,
    )
    .map_err(detail)?;
    peritus_sandbox_macos::MacosBackend::new(&probe, config).map_err(detail)
}

#[cfg(target_os = "windows")]
pub(super) fn open(
    config: &LocalProcessConfig,
    directory: &Path,
) -> Result<impl NativeSandboxBackend, String> {
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
    let workspace =
        peritus_sandbox_windows::WindowsPath::new(super::sandbox::normalized_path(directory)?)
            .map_err(detail)?;
    let inputs = [&config.executable, &config.model_path]
        .into_iter()
        .map(|input| {
            let input = input.canonicalize().map_err(|_| "resolve installed inference input")?;
            peritus_sandbox_windows::WindowsPath::new(super::sandbox::normalized_path(&input)?)
                .map_err(detail)
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
    peritus_sandbox_windows::WindowsBackend::new(config).map_err(detail)
}
