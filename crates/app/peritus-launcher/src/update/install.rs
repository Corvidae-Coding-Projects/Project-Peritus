//! Transactional native installer invocation and installed-version verification.

use std::{env, path::PathBuf, process::Command};

use crate::LauncherError;

use super::release::Release;

pub(super) fn apply(package: &std::path::Path, release: &Release) -> Result<(), LauncherError> {
    let target = installed_command()?;
    if cfg!(windows) {
        deferred_windows(package, release, &target)
    } else {
        immediate_unix(package, release, &target)
    }
}

#[cfg(not(windows))]
fn immediate_unix(
    package: &std::path::Path,
    release: &Release,
    target: &std::path::Path,
) -> Result<(), LauncherError> {
    let script =
        package.join(if target.exists() { "Upgrade-Peritus.sh" } else { "Install-Peritus.sh" });
    let status = Command::new("sh")
        .arg(&script)
        .arg(package)
        .status()
        .map_err(|error| LauncherError::Update(format!("start native updater: {error}")))?;
    if !status.success() {
        return Err(LauncherError::Update(format!("native updater failed with status {status}")));
    }
    verify(target, release)
}

#[cfg(windows)]
fn immediate_unix(
    _package: &std::path::Path,
    _release: &Release,
    _target: &std::path::Path,
) -> Result<(), LauncherError> {
    unreachable!()
}

#[cfg(windows)]
fn deferred_windows(
    package: &std::path::Path,
    release: &Release,
    target: &std::path::Path,
) -> Result<(), LauncherError> {
    use std::os::windows::process::CommandExt as _;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let helper = package.parent().unwrap_or(package).join("finish-update.ps1");
    let installer =
        package.join(if target.exists() { "Upgrade-Peritus.ps1" } else { "Install-Peritus.ps1" });
    let script = format!(
        "$ErrorActionPreference='Stop'\nWait-Process -Id {} -Timeout 120 -ErrorAction SilentlyContinue\n& '{}' -BundleRoot '{}'\n$version = & '{}' --version\nif ($version -ne 'peritus {}') {{ throw 'installed version verification failed' }}\nRemove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue\n",
        std::process::id(),
        quote(&installer),
        quote(package),
        quote(target),
        release.version()
    );
    std::fs::write(&helper, script).map_err(|error| {
        LauncherError::filesystem("write deferred update helper", &helper, error)
    })?;
    Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&helper)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|error| {
            LauncherError::Update(format!("start deferred native updater: {error}"))
        })?;
    Ok(())
}

#[cfg(not(windows))]
fn deferred_windows(
    _package: &std::path::Path,
    _release: &Release,
    _target: &std::path::Path,
) -> Result<(), LauncherError> {
    unreachable!()
}

fn installed_command() -> Result<PathBuf, LauncherError> {
    if cfg!(windows) {
        environment("LOCALAPPDATA").map(|root| root.join("Programs/Peritus/bin/peritus.exe"))
    } else {
        environment("HOME").map(|root| root.join(".local/bin/peritus"))
    }
}

fn environment(name: &'static str) -> Result<PathBuf, LauncherError> {
    let value =
        env::var_os(name).ok_or_else(|| LauncherError::Update(format!("{name} is unavailable")))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(LauncherError::Update(format!("{name} must contain an absolute path")));
    }
    Ok(path)
}

#[cfg(not(windows))]
fn verify(target: &std::path::Path, release: &Release) -> Result<(), LauncherError> {
    let output = Command::new(target)
        .arg("--version")
        .output()
        .map_err(|error| LauncherError::Update(format!("run installed version check: {error}")))?;
    let expected = format!("peritus {}\n", release.version());
    if output.status.success() && output.stdout == expected.as_bytes() {
        Ok(())
    } else {
        Err(LauncherError::Update("installed version verification failed".to_owned()))
    }
}

#[cfg(windows)]
fn quote(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
