//! Product package build, installation, and native lifecycle qualification.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::XtaskError;

pub(crate) fn build(root: &Path) -> Result<PathBuf, XtaskError> {
    assemble(root, false)?;
    Ok(package_path(root))
}

pub(crate) fn install(root: &Path) -> Result<PathBuf, XtaskError> {
    let package = build(root)?;
    run_installer(&package, None, NativeAction::Install)?;
    Ok(package)
}

pub(crate) fn smoke(root: &Path) -> Result<PathBuf, XtaskError> {
    build_debug_binaries(root)?;
    assemble(root, true)?;
    let package = package_path(root);
    let subject = SmokeSubject::new()?;
    let (state_root, executable) = smoke_paths(subject.path());
    fs::create_dir_all(&state_root)
        .map_err(|error| XtaskError::io("create native smoke state root at", &state_root, error))?;
    let marker = state_root.join("qualification.txt");
    fs::write(&marker, b"preserve-me\n")
        .map_err(|error| XtaskError::io("write native smoke state marker at", &marker, error))?;

    run_installer(&package, Some(subject.path()), NativeAction::Install)?;
    run_installed_version(&executable, subject.path())?;
    run_installer(&package, Some(subject.path()), NativeAction::Upgrade)?;
    run_installed_version(&executable, subject.path())?;
    run_installer(&package, Some(subject.path()), NativeAction::Uninstall)?;

    if executable.exists() {
        return Err(XtaskError::metadata("native product uninstall retained the peritus command"));
    }
    let preserved = fs::read(&marker)
        .map_err(|error| XtaskError::io("read native smoke state marker at", &marker, error))?;
    if preserved != b"preserve-me\n" {
        return Err(XtaskError::metadata("native product lifecycle changed protected state"));
    }
    Ok(package)
}

fn build_debug_binaries(root: &Path) -> Result<(), XtaskError> {
    let helper_package = match host_os() {
        "linux" => "peritus-sandbox-linux",
        "macos" => "peritus-sandbox-macos",
        "windows" => "peritus-sandbox-windows",
        _ => return Err(XtaskError::metadata("native product packaging is unsupported here")),
    };
    let status = Command::new("cargo")
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "2")
        .args([
            "build",
            "--locked",
            "-p",
            "peritus-cli",
            "-p",
            "peritus-daemon",
            "-p",
            "peritus-tui",
            "-p",
            helper_package,
        ])
        .status()
        .map_err(|error| XtaskError::io("build native product smoke binaries in", root, error))?;
    require_success(status.success(), "native product smoke binary build failed")
}

fn assemble(root: &Path, use_debug_artifacts: bool) -> Result<(), XtaskError> {
    let mut command = Command::new("cargo");
    command.current_dir(root).env("CARGO_BUILD_JOBS", "2").args([
        "run",
        "--locked",
        "-p",
        "peritus-platform-qualification",
        "--bin",
        "peritus-package",
        "--",
    ]);
    if use_debug_artifacts {
        command.arg("--use-debug-artifacts");
    }
    let status = command
        .status()
        .map_err(|error| XtaskError::io("start product package builder in", root, error))?;
    require_success(status.success(), "product package builder failed")
}

#[derive(Clone, Copy)]
enum NativeAction {
    Install,
    Upgrade,
    Uninstall,
}

fn run_installer(
    package: &Path,
    subject: Option<&Path>,
    action: NativeAction,
) -> Result<(), XtaskError> {
    let stem = match action {
        NativeAction::Install => "Install-Peritus",
        NativeAction::Upgrade => "Upgrade-Peritus",
        NativeAction::Uninstall => "Uninstall-Peritus",
    };
    let mut command = if cfg!(windows) {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        command.arg(package.join(format!("{stem}.ps1")));
        if !matches!(action, NativeAction::Uninstall) {
            command.arg("-BundleRoot").arg(package);
        }
        command
    } else {
        let mut command = Command::new("sh");
        command.arg(package.join(format!("{stem}.sh")));
        if !matches!(action, NativeAction::Uninstall) {
            command.arg(package);
        }
        command
    };
    if let Some(subject) = subject {
        if cfg!(windows) {
            command.env("LOCALAPPDATA", subject);
        } else {
            command.env("HOME", subject);
        }
    }
    let status = command.status().map_err(|error| {
        XtaskError::io("run native product lifecycle adapter from", package, error)
    })?;
    require_success(status.success(), "native product lifecycle adapter failed")
}

fn run_installed_version(executable: &Path, subject: &Path) -> Result<(), XtaskError> {
    let mut command = Command::new(executable);
    command.arg("--version");
    if cfg!(windows) {
        command.env("LOCALAPPDATA", subject);
    } else {
        command.env("HOME", subject);
    }
    let status = command
        .status()
        .map_err(|error| XtaskError::io("run installed product command at", executable, error))?;
    require_success(status.success(), "installed peritus command failed")
}

fn package_path(root: &Path) -> PathBuf {
    root.join("dist").join(format!("peritus-{}-{}", host_os(), std::env::consts::ARCH))
}

fn smoke_paths(subject: &Path) -> (PathBuf, PathBuf) {
    match host_os() {
        "linux" => (subject.join(".local/state/peritus"), subject.join(".local/bin/peritus")),
        "macos" => (
            subject.join("Library/Application Support/Peritus/state"),
            subject.join(".local/bin/peritus"),
        ),
        "windows" => {
            (subject.join("Peritus/state"), subject.join("Programs/Peritus/bin/peritus.exe"))
        }
        _ => (subject.join("state"), subject.join("peritus")),
    }
}

fn require_success(success: bool, detail: &'static str) -> Result<(), XtaskError> {
    if success { Ok(()) } else { Err(XtaskError::metadata(detail)) }
}

struct SmokeSubject {
    path: PathBuf,
}

impl SmokeSubject {
    fn new() -> Result<Self, XtaskError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| XtaskError::metadata("system clock is before the Unix epoch"))?
            .as_nanos();
        let path = std::env::temp_dir()
            .join(format!("peritus-native-smoke-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).map_err(|error| {
            XtaskError::io("create native product smoke subject at", &path, error)
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SmokeSubject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const fn host_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unsupported"
    }
}
