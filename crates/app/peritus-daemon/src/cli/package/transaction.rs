//! Serialized package transaction ownership without elapsed-time expiry.

use std::{ffi::OsString, fs::OpenOptions, path::PathBuf, process::{Command, ExitCode}};

pub(super) fn locked_run(lock: OsString, mut command: Vec<OsString>) -> ExitCode {
    match locked_run_inner(PathBuf::from(lock), &mut command) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            super::super::write_error(&format!("package transaction lock failed: {error}"));
            ExitCode::FAILURE
        }
    }
}

fn locked_run_inner(lock: PathBuf, command: &mut Vec<OsString>) -> Result<bool, String> {
    if !lock.is_absolute()
        || lock.file_name().and_then(|name| name.to_str()) != Some("install.lock")
        || lock
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            != Some("transactions")
        || lock
            .parent()
            .and_then(std::path::Path::parent)
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            != Some(".install")
    {
        return Err("lock path is outside a package transaction store".to_owned());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options
        .open(&lock)
        .map_err(|error| format!("open {}: {error}", lock.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("protect {}: {error}", lock.display()))?;
    }
    file.lock().map_err(|error| format!("acquire {}: {error}", lock.display()))?;
    let program = command.remove(0);
    let status = Command::new(program)
        .args(command)
        .env("PERITUS_INSTALL_LOCKED", "1")
        .status()
        .map_err(|error| format!("run locked installer: {error}"))?;
    file.unlock().map_err(|error| format!("release {}: {error}", lock.display()))?;
    Ok(status.success())
}
