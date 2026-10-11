//! Product package build and installation.

mod host_version;

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::XtaskError;

pub(crate) fn build(root: &Path) -> Result<PathBuf, XtaskError> {
    assemble(root, false)?;
    Ok(package_path(root))
}

pub(crate) fn build_from_release_artifacts(root: &Path) -> Result<PathBuf, XtaskError> {
    assemble_prepared_release(root)?;
    Ok(package_path(root))
}

pub(crate) fn install(root: &Path) -> Result<PathBuf, XtaskError> {
    let package = build(root)?;
    run_installer(&package, None, NativeAction::Install)?;
    Ok(package)
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

fn assemble_prepared_release(root: &Path) -> Result<(), XtaskError> {
    let status = Command::new("cargo")
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "2")
        .args([
            "run",
            "--locked",
            "-p",
            "peritus-platform-qualification",
            "--bin",
            "peritus-package",
            "--",
            "--use-release-artifacts",
        ])
        .status()
        .map_err(|error| XtaskError::io("assemble prepared release package in", root, error))?;
    require_success(status.success(), "prepared release package assembly failed")
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

pub(crate) fn package_path(root: &Path) -> PathBuf {
    root.join("dist").join(format!("peritus-{}-{}", host_os(), std::env::consts::ARCH))
}

fn require_success(success: bool, detail: &'static str) -> Result<(), XtaskError> {
    if success { Ok(()) } else { Err(XtaskError::metadata(detail)) }
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
