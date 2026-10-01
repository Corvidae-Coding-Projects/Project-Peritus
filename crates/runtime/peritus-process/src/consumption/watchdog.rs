//! Platform-specific crash-watchdog configuration.

use std::path::{Path, PathBuf};

use crate::ProcessError;

#[cfg(target_os = "linux")]
pub(super) fn configured_crash_watchdog(
    path: Option<&Path>,
    registry_root: &Path,
    workspace_root: &Path,
) -> Result<Option<PathBuf>, ProcessError> {
    path.map(|path| validate_crash_watchdog(path, registry_root, workspace_root)).transpose()
}

#[cfg(not(target_os = "linux"))]
pub(super) const fn configured_crash_watchdog(
    path: Option<&Path>,
    _registry_root: &Path,
    _workspace_root: &Path,
) -> Result<Option<PathBuf>, ProcessError> {
    if path.is_some() {
        return Err(super::errors::store_error(
            "process crash watchdog is only supported on Linux",
        ));
    }
    Ok(None)
}

#[cfg(target_os = "linux")]
fn validate_crash_watchdog(
    path: &Path,
    registry_root: &Path,
    workspace_root: &Path,
) -> Result<PathBuf, ProcessError> {
    use std::os::unix::fs::PermissionsExt as _;

    if !path.is_absolute() {
        return Err(super::errors::store_error("process crash watchdog path is not absolute"));
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| super::errors::store_error("process crash watchdog cannot be inspected"))?;
    if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(super::errors::store_error(
            "process crash watchdog is not an executable regular file",
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| {
        super::errors::store_error("process crash watchdog cannot be canonicalized")
    })?;
    if canonical != path
        || canonical.starts_with(registry_root)
        || canonical.starts_with(workspace_root)
    {
        return Err(super::errors::store_error(
            "process crash watchdog is aliased or overlaps protected mutable state",
        ));
    }
    Ok(canonical)
}
