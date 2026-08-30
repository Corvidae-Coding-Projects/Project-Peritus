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

pub(crate) fn qualify(root: &Path) -> Result<PathBuf, XtaskError> {
    build_debug_binaries(root)?;
    build_h2_binaries(root)?;
    assemble(root, true)?;
    let package = package_path(root);
    let run_root = qualification_run_root(root)?;
    let scratch = run_root.join("scratch");
    let artifacts = run_root.join("artifacts");
    let report = run_root.join("report.json");
    fs::create_dir_all(&scratch).map_err(|error| {
        XtaskError::io("create H2 qualification scratch root at", &scratch, error)
    })?;
    fs::create_dir_all(&artifacts).map_err(|error| {
        XtaskError::io("create H2 qualification artifact root at", &artifacts, error)
    })?;
    let status = Command::new(debug_binary(root, "peritus-h2"))
        .current_dir(root)
        .args(["--controller"])
        .arg(debug_binary(root, "peritus-h2-controller"))
        .args(["--package"])
        .arg(&package)
        .args(["--manifest"])
        .arg(package.join("manifest.toml"))
        .args(["--scratch"])
        .arg(&scratch)
        .args(["--artifacts"])
        .arg(&artifacts)
        .args(["--report"])
        .arg(&report)
        .args(["--platform", host_os(), "--architecture", std::env::consts::ARCH])
        .args(["--version", &host_version()?])
        .status()
        .map_err(|error| {
            XtaskError::io("run complete native H2 qualification from", &package, error)
        })?;
    require_success(status.success(), "native H2 qualification did not reach Ready")?;
    if !report.is_file() {
        return Err(XtaskError::metadata("native H2 qualification did not retain its report"));
    }
    Ok(report)
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

fn build_h2_binaries(root: &Path) -> Result<(), XtaskError> {
    let status = Command::new("cargo")
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "2")
        .args([
            "build",
            "--locked",
            "--package",
            "peritus-platform-qualification",
            "--bin",
            "peritus-h2",
            "--bin",
            "peritus-h2-controller",
        ])
        .status()
        .map_err(|error| XtaskError::io("build native H2 controllers in", root, error))?;
    require_success(status.success(), "native H2 controller build failed")
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

fn debug_binary(root: &Path, name: &str) -> PathBuf {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    root.join("target").join("debug").join(format!("{name}{suffix}"))
}

fn qualification_run_root(root: &Path) -> Result<PathBuf, XtaskError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| XtaskError::metadata("system clock is before the Unix epoch"))?
        .as_nanos();
    Ok(root.join("target/peritus-qualification/h2").join(format!(
        "{}-{}-{nonce}",
        host_os(),
        std::env::consts::ARCH
    )))
}

fn host_version() -> Result<String, XtaskError> {
    match host_os() {
        "linux" => {
            let raw = fs::read_to_string("/proc/sys/kernel/osrelease").map_err(|error| {
                XtaskError::io(
                    "read Linux kernel version from",
                    Path::new("/proc/sys/kernel/osrelease"),
                    error,
                )
            })?;
            normalize_version(&raw)
                .ok_or_else(|| XtaskError::metadata("Linux kernel version is malformed"))
        }
        "macos" => command_version("/usr/bin/sw_vers", &["-productVersion"], false),
        "windows" => command_version(
            "powershell",
            &["-NoProfile", "-Command", "[Environment]::OSVersion.Version.ToString()"],
            true,
        ),
        _ => Err(XtaskError::metadata("native H2 qualification is unsupported here")),
    }
}

fn command_version(
    executable: &str,
    arguments: &[&str],
    windows_marketing_version: bool,
) -> Result<String, XtaskError> {
    let output = Command::new(executable).args(arguments).output().map_err(|error| {
        XtaskError::io("run host-version probe with", Path::new(executable), error)
    })?;
    if !output.status.success() {
        return Err(XtaskError::metadata("host-version probe failed"));
    }
    let raw = String::from_utf8(output.stdout)
        .map_err(|_| XtaskError::metadata("host-version probe returned non-UTF-8 output"))?;
    let version = if windows_marketing_version {
        normalize_windows_version(&raw)
    } else {
        normalize_version(&raw)
    };
    version.ok_or_else(|| XtaskError::metadata("host-version probe returned malformed output"))
}

fn normalize_windows_version(raw: &str) -> Option<String> {
    let fields = numeric_version_fields(raw)?;
    let build = *fields.get(2)?;
    if fields.first() == Some(&10) && build >= 22_000 {
        Some(format!("11.0.0.{build}"))
    } else {
        normalized_fields(fields)
    }
}

fn normalize_version(raw: &str) -> Option<String> {
    normalized_fields(numeric_version_fields(raw)?)
}

fn numeric_version_fields(raw: &str) -> Option<Vec<u32>> {
    let mut fields = Vec::new();
    for field in raw.trim().split('.').take(4) {
        let digits = field.chars().take_while(char::is_ascii_digit).collect::<String>();
        if digits.is_empty() {
            break;
        }
        fields.push(digits.parse().ok()?);
    }
    (!fields.is_empty()).then_some(fields)
}

fn normalized_fields(mut fields: Vec<u32>) -> Option<String> {
    if fields.len() > 4 {
        return None;
    }
    while fields.len() < 3 {
        fields.push(0);
    }
    Some(fields.iter().map(u32::to_string).collect::<Vec<_>>().join("."))
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

#[cfg(test)]
mod tests {
    use super::{normalize_version, normalize_windows_version};

    #[test]
    fn native_versions_are_reduced_to_the_h2_contract() {
        assert_eq!(normalize_version("7.1.8-200.fc44.x86_64\n").as_deref(), Some("7.1.8"));
        assert_eq!(normalize_version("15.7.1\n").as_deref(), Some("15.7.1"));
        assert_eq!(normalize_version("15\n").as_deref(), Some("15.0.0"));
        assert_eq!(normalize_version("unknown"), None);
    }

    #[test]
    fn windows_kernel_build_is_projected_to_the_supported_product_version() {
        assert_eq!(normalize_windows_version("10.0.26100.0\r\n").as_deref(), Some("11.0.0.26100"));
        assert_eq!(normalize_windows_version("11.0.0.30000\n").as_deref(), Some("11.0.0.30000"));
    }
}
