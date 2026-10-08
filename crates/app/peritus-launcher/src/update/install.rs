//! Durable native installer ownership and complete installed-pair verification.

use std::{env, path::{Path, PathBuf}, process::Command};

use crate::LauncherError;

use super::{download::Package, release::Release};
#[cfg(windows)]
use super::download::OwnerProgress;

#[cfg(windows)]
const UPDATE_OWNER_FLAG: &str = "--peritus-product-update-owner";

pub(super) fn apply(package: &mut Package, release: &Release) -> Result<(), LauncherError> {
    if package.is_installed() {
        return verify_installed_pair(package, release);
    }
    #[cfg(windows)]
    {
        deferred_windows(package)
    }
    #[cfg(not(windows))]
    {
        immediate_unix(package, release)
    }
}

#[cfg(not(windows))]
fn immediate_unix(package: &mut Package, release: &Release) -> Result<(), LauncherError> {
    package.ensure_extracted()?;
    let upgrade = installed_pair_exists()?;
    let bundle = package.bundle_path();
    let script = bundle.join(if upgrade { "Upgrade-Peritus.sh" } else { "Install-Peritus.sh" });
    let status = match super::process::status(
        Command::new("sh").arg(&script).arg(&bundle),
        "run native updater",
        |identity| package.record_installing(identity),
    ) {
        Ok(status) => status,
        Err(error) => {
            package.record_install_failed(error.to_string())?;
            return Err(error);
        }
    };
    if !status.success() {
        let error = LauncherError::Update(format!("native updater failed with status {status}"));
        package.record_install_failed(error.to_string())?;
        return Err(error);
    }
    if let Err(error) = verify_installed_pair(package, release) {
        package.record_install_failed(error.to_string())?;
        return Err(error);
    }
    package.record_installed()
}

#[cfg(windows)]
fn deferred_windows(package: &mut Package) -> Result<(), LauncherError> {
    use std::process::Stdio;
    use std::time::Duration;

    use peritus_process::NativeWindowsProcessOwner;

    package.prepare_windows_owner()?;
    let receipt = package.receipt_path().to_owned();
    let operation = package.operation_id().to_owned();
    package.release_lock();
    let executable = std::env::current_exe().map_err(|error| {
        LauncherError::Update(format!("resolve Windows update owner executable: {error}"))
    })?;
    let mut command = Command::new(executable);
    command
        .arg(UPDATE_OWNER_FLAG)
        .arg(&receipt)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    NativeWindowsProcessOwner::configure_detached_command(&mut command);
    let mut child = command.spawn().map_err(|error| {
        LauncherError::Update(format!("start durable Windows update owner: {error}"))
    })?;
    loop {
        match super::download::observe_owner_progress(&receipt, &operation)? {
            OwnerProgress::AwaitingIdentity => {}
            OwnerProgress::ExactOwned | OwnerProgress::Installed => return Ok(()),
        }
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                return match super::download::observe_owner_progress(&receipt, &operation) {
                    Ok(OwnerProgress::Installed) => Ok(()),
                    Ok(_) => Err(LauncherError::Update(format!(
                        "Windows update owner exited with status {status} before exact adoption"
                    ))),
                    Err(error) => Err(error),
                };
            }
            Err(error) => {
                return Err(LauncherError::Update(format!(
                    "observe Windows update owner child: {error}"
                )));
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(windows)]
pub fn update_owner_argument() -> Option<PathBuf> {
    let mut arguments = std::env::args_os();
    let _program = arguments.next()?;
    if arguments.next()?.to_str()? != UPDATE_OWNER_FLAG {
        return None;
    }
    let path = PathBuf::from(arguments.next()?);
    arguments.next().is_none().then_some(path)
}

#[cfg(windows)]
pub fn run_update_owner(path: &Path) -> Result<(), LauncherError> {
    use peritus_process::NativeWindowsProcessOwner;

    let mut package = Package::reopen_owner(path)?;
    let release = package.release().clone();
    let owner = NativeWindowsProcessOwner::activate_current(
        package.windows_job_identity(),
        package.windows_job_name(),
    )
    .map_err(|error| LauncherError::Update(format!("activate Windows update owner: {error}")))?;
    package.record_windows_owner(owner.identity())?;
    let result = (|| {
        package.ensure_extracted()?;
        immediate_windows(&mut package, &release)
    })();
    if let Err(error) = &result {
        package.record_install_failed(error.to_string())?;
    }
    result
}

#[cfg(windows)]
fn immediate_windows(package: &mut Package, release: &Release) -> Result<(), LauncherError> {
    let upgrade = installed_pair_exists()?;
    let bundle = package.bundle_path();
    let script = bundle.join(if upgrade { "Upgrade-Peritus.ps1" } else { "Install-Peritus.ps1" });
    let status = match super::process::status(
        Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .arg("-BundleRoot")
            .arg(&bundle),
        "run native updater",
        |identity| package.record_installing(identity),
    ) {
        Ok(status) => status,
        Err(error) => {
            package.record_install_failed(error.to_string())?;
            return Err(error);
        }
    };
    if !status.success() {
        let error = LauncherError::Update(format!("native updater failed with status {status}"));
        package.record_install_failed(error.to_string())?;
        return Err(error);
    }
    if let Err(error) = verify_installed_pair(package, release) {
        package.record_install_failed(error.to_string())?;
        return Err(error);
    }
    package.record_installed()
}

fn installed_pair_exists() -> Result<bool, LauncherError> {
    let (application, daemon) = installed_commands()?;
    match (application.is_file(), daemon.is_file()) {
        (false, false) => Ok(false),
        (true, true) => Ok(true),
        _ => Err(LauncherError::Update(format!(
            "installed package is partial; expected both {} and {}",
            application.display(),
            daemon.display()
        ))),
    }
}

fn installed_commands() -> Result<(PathBuf, PathBuf), LauncherError> {
    let root = if cfg!(windows) {
        environment("LOCALAPPDATA")?.join("Programs/Peritus/bin")
    } else if cfg!(target_os = "macos") {
        environment("HOME")?.join("Library/Application Support/Peritus/bin")
    } else {
        environment("HOME")?.join(".local/bin")
    };
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    Ok((root.join(format!("peritus{suffix}")), root.join(format!("peritusd{suffix}"))))
}

fn environment(name: &'static str) -> Result<PathBuf, LauncherError> {
    let value = env::var_os(name)
        .ok_or_else(|| LauncherError::Update(format!("{name} is unavailable")))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(LauncherError::Update(format!("{name} must contain an absolute path")));
    }
    Ok(path)
}

fn verify_installed_pair(
    package: &Package,
    release: &Release,
) -> Result<(), LauncherError> {
    let (application, daemon) = installed_commands()?;
    if !application.is_file() || !daemon.is_file() {
        return Err(LauncherError::Update(
            "native updater did not publish the complete application and daemon pair".to_owned(),
        ));
    }
    verify_version(
        &application,
        "peritus",
        release,
        &package.capture_path("installed-peritus.version"),
    )?;
    verify_version(
        &daemon,
        "peritusd",
        release,
        &package.capture_path("installed-peritusd.version"),
    )
}

fn verify_version(
    command: &Path,
    name: &'static str,
    release: &Release,
    capture: &Path,
) -> Result<(), LauncherError> {
    let (status, stdout) = super::process::stdout(
        Command::new(command).arg("--version"),
        "run installed version check",
        capture,
    )?;
    let expected = format!("{name} {}\n", release.version());
    if status.success() && stdout == expected.as_bytes() {
        Ok(())
    } else {
        Err(LauncherError::Update(format!(
            "installed {name} version verification failed"
        )))
    }
}
