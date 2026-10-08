//! Atomic package-generation adoption beneath one validated installation root.

mod filesystem;
mod transaction;

use filesystem::{atomic_replace, is_filesystem_link, remove_filesystem_link, sync_parent};

use std::{
    ffi::OsString,
    fs::OpenOptions,
    path::PathBuf,
    process::ExitCode,
};

pub(super) fn locked_run(lock: OsString, command: Vec<OsString>) -> ExitCode {
    transaction::locked_run(lock, command)
}

pub(super) fn adopt(
    store: OsString,
    candidate: OsString,
    current: OsString,
    configuration: Option<OsString>,
    retired: Option<OsString>,
) -> ExitCode {
    match adopt_guarded(
        PathBuf::from(store),
        PathBuf::from(candidate),
        PathBuf::from(current),
        configuration,
        retired.map(PathBuf::from),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            super::write_error(&format!("package generation adoption failed: {error}"));
            ExitCode::FAILURE
        }
    }
}

pub(super) fn remove(
    store: OsString,
    current: OsString,
    expected: OsString,
    configuration: Option<OsString>,
) -> ExitCode {
    match remove_guarded(
        PathBuf::from(store),
        PathBuf::from(current),
        PathBuf::from(expected),
        configuration,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            super::write_error(&format!("package generation removal failed: {error}"));
            ExitCode::FAILURE
        }
    }
}

fn remove_guarded(
    store: PathBuf,
    current: PathBuf,
    expected: PathBuf,
    configuration: Option<OsString>,
) -> Result<(), String> {
    let _guard = configuration.map(acquire_daemon_guard).transpose()?;
    let generation_root = store.join("generations");
    if !store.is_absolute()
        || current != store.join("current")
        || expected.parent() != Some(generation_root.as_path())
    {
        return Err("generation removal paths are outside the validated store".to_owned());
    }
    let generations = std::fs::canonicalize(generation_root)
        .map_err(|error| format!("resolve generation store: {error}"))?;
    let expected = std::fs::canonicalize(&expected)
        .map_err(|error| format!("resolve expected generation: {error}"))?;
    if expected.parent() != Some(generations.as_path()) || !expected.is_dir() {
        return Err("expected removal target is not a retained generation".to_owned());
    }
    let metadata = match std::fs::symlink_metadata(&current) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("inspect current generation pointer: {error}")),
    };
    if !is_filesystem_link(&metadata) {
        return Err("current generation pointer is not a filesystem link".to_owned());
    }
    let observed = std::fs::canonicalize(&current)
        .map_err(|error| format!("resolve current generation: {error}"))?;
    if observed != expected {
        return Err("current generation changed before rollback removal".to_owned());
    }
    remove_filesystem_link(&current)
        .map_err(|error| format!("remove current generation pointer: {error}"))?;
    sync_parent(&store).map_err(|error| format!("synchronize generation removal: {error}"))
}

fn adopt_guarded(
    store: PathBuf,
    candidate: PathBuf,
    current: PathBuf,
    configuration: Option<OsString>,
    retired: Option<PathBuf>,
) -> Result<(), String> {
    let _guard = configuration.map(acquire_daemon_guard).transpose()?;
    let retired_moved = retired
        .as_deref()
        .map(|retired| retire_legacy_path(&store, &current, retired))
        .transpose()?
        .unwrap_or(false);
    match adopt_inner(store, candidate, current.clone()) {
        Ok(()) => Ok(()),
        Err(error) if retired_moved => {
            let Some(retired) = retired else {
                return Err(format!("{error}; retained legacy path was not recorded"));
            };
            match restore_retired_path(&current, &retired) {
                Ok(()) => Err(error),
                Err(restore) => Err(format!(
                    "{error}; restoring retained legacy path {} also failed: {restore}",
                    retired.display(),
                )),
            }
        }
        Err(error) => Err(error),
    }
}

fn retire_legacy_path(
    store: &std::path::Path,
    current: &std::path::Path,
    retired: &std::path::Path,
) -> Result<bool, String> {
    let current_name = current.file_name().and_then(|name| name.to_str());
    let transaction_root = store.join("transactions");
    if !retired.is_absolute()
        || retired.parent() != Some(transaction_root.as_path())
        || !retired
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("retired-"))
        || current.parent() != store.parent()
        || !current_name.is_some_and(|name| matches!(name, "bin" | "libexec" | "share"))
    {
        return Err("retired package path is outside the validated transaction store".to_owned());
    }
    match std::fs::symlink_metadata(current) {
        Ok(metadata) if is_filesystem_link(&metadata) => Ok(false),
        Ok(metadata) if metadata.is_dir() => {
            if retired.exists() {
                return Err("live and retired legacy package directories both exist".to_owned());
            }
            std::fs::rename(current, retired).map_err(|error| {
                format!("retain legacy directory {}: {error}", current.display())
            })?;
            Ok(true)
        }
        Ok(_) => Err("legacy package path is not a directory".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && retired.is_dir() => {
            Ok(false)
        }
        Err(error) => Err(format!(
            "inspect legacy package path {}: {error}",
            current.display(),
        )),
    }
}

fn restore_retired_path(
    current: &std::path::Path,
    retired: &std::path::Path,
) -> Result<(), String> {
    if let Ok(metadata) = std::fs::symlink_metadata(current) {
        let removal = if metadata.file_type().is_symlink() {
            std::fs::remove_file(current)
        } else {
            std::fs::remove_dir(current)
        };
        removal.map_err(|error| format!("remove failed adopted link: {error}"))?;
    }
    std::fs::rename(retired, current).map_err(|error| format!("restore legacy directory: {error}"))
}

fn acquire_daemon_guard(configuration: OsString) -> Result<std::fs::File, String> {
    let config = crate::DaemonConfig::load(configuration)
        .map_err(|error| format!("load daemon publication guard configuration: {error}"))?;
    let state_root = config.paths().state_root();
    std::fs::create_dir_all(state_root)
        .map_err(|error| format!("create {}: {error}", state_root.display()))?;
    let path = state_root.join("daemon.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|error| format!("open {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("protect {}: {error}", path.display()))?;
    }
    file.try_lock().map_err(|error| {
        format!(
            "acquire exact daemon publication guard {}: {error}; repeat the handoff",
            path.display(),
        )
    })?;
    Ok(file)
}

fn adopt_inner(store: PathBuf, candidate: PathBuf, current: PathBuf) -> Result<(), String> {
    if !store.is_absolute() || !candidate.is_absolute() || !current.is_absolute() {
        return Err("generation links must be absolute".to_owned());
    }
    let candidate_parent = candidate
        .parent()
        .ok_or_else(|| "candidate generation link has no parent".to_owned())?;
    if current.parent() != Some(candidate_parent)
        || !candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".current."))
    {
        return Err("generation links are outside one stable link parent".to_owned());
    }
    let metadata = std::fs::symlink_metadata(&candidate)
        .map_err(|error| format!("inspect candidate generation link: {error}"))?;
    if !is_filesystem_link(&metadata) {
        return Err("candidate generation pointer is not a filesystem link".to_owned());
    }
    let target = std::fs::canonicalize(&candidate)
        .map_err(|error| format!("resolve candidate generation: {error}"))?;
    let generations = std::fs::canonicalize(store.join("generations"))
        .map_err(|error| format!("resolve generation store: {error}"))?;
    let current_name = current
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "stable generation link name is not UTF-8".to_owned())?;
    let valid_target = if current == store.join("current") {
        target.parent() == Some(generations.as_path())
    } else if matches!(current_name, "bin" | "libexec" | "share")
        && current.parent() == store.parent()
    {
        let target_name = target.file_name().and_then(|name| name.to_str());
        (target_name == Some(current_name)
            || (current_name == "share" && target_name == Some("stable-share")))
            && target.parent().and_then(std::path::Path::parent) == Some(generations.as_path())
    } else {
        false
    };
    if !valid_target || !target.is_dir() {
        return Err("candidate does not name an allowed retained-generation path".to_owned());
    }
    if current == store.join("current")
        && let Ok(metadata) = std::fs::symlink_metadata(&current)
        && !is_filesystem_link(&metadata)
    {
        return Err("current generation pointer is not a filesystem link".to_owned());
    }
    atomic_replace(&candidate, &current)
        .map_err(|error| format!("atomically replace current generation: {error}"))?;
    sync_parent(candidate_parent)
        .map_err(|error| format!("synchronize current generation: {error}"))
}
