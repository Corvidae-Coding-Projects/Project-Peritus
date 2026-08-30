//! Private native installation paths and bounded child commands.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::Platform;

use super::super::args::ControllerPaths;
use super::super::request::BoundRequest;

const MAX_CHILD_OUTPUT: usize = 2 * 1024 * 1024;

pub(super) struct HostLayout {
    pub(super) platform: Platform,
    pub(super) cli: PathBuf,
    pub(super) daemon: PathBuf,
    pub(super) tui: PathBuf,
    pub(super) helper: PathBuf,
    pub(super) service: PathBuf,
    pub(super) config: PathBuf,
    pub(super) state: PathBuf,
    pub(super) logs: PathBuf,
}

impl HostLayout {
    pub(super) fn new(
        paths: &ControllerPaths,
        request: &BoundRequest,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let home = paths.subject_root.clone();
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let layout = match request.document.target().platform_name() {
            "linux" => Self {
                platform: Platform::Linux,
                cli: home.join(".local/bin/peritus"),
                daemon: home.join(".local/bin/peritusd"),
                tui: home.join(".local/bin/peritus-tui"),
                helper: home.join(".local/libexec/peritus/peritus-linux-sandbox-helper"),
                service: home.join(".local/share/peritus/peritus.service"),
                config: home.join(".config/peritus/peritus.toml"),
                state: home.join(".local/state/peritus"),
                logs: home.join(".local/state/peritus/log"),
            },
            "macos" => {
                let application = home.join("Library/Application Support/Peritus");
                Self {
                    platform: Platform::Macos,
                    cli: application.join(format!("bin/peritus{suffix}")),
                    daemon: application.join(format!("bin/peritusd{suffix}")),
                    tui: application.join(format!("bin/peritus-tui{suffix}")),
                    helper: application.join("libexec/peritus-macos-sandbox-helper"),
                    service: application.join("share/peritus/com.corvidae.peritus.plist.in"),
                    config: application.join("config/peritus.toml"),
                    state: application.join("state"),
                    logs: home.join("Library/Logs/Peritus"),
                }
            }
            "windows" => {
                let local = home.join("local-app-data");
                let program = local.join("Programs/Peritus");
                Self {
                    platform: Platform::Windows,
                    cli: program.join("bin/peritus.exe"),
                    daemon: program.join("bin/peritusd.exe"),
                    tui: program.join("bin/peritus-tui.exe"),
                    helper: program.join("libexec/peritus-windows-sandbox-helper.exe"),
                    service: program.join("share/Peritus.Task.xml.in"),
                    config: local.join("Peritus/config/peritus.toml"),
                    state: local.join("Peritus/state"),
                    logs: local.join("Peritus/logs"),
                }
            }
            _ => return Err("H2 request target is unsupported".into()),
        };
        Ok(layout)
    }

    pub(super) fn package_files(&self) -> [&Path; 5] {
        [&self.cli, &self.daemon, &self.tui, &self.helper, &self.service]
    }

    pub(super) fn protected_roots(&self) -> [&Path; 3] {
        [&self.config, &self.state, &self.logs]
    }
}

#[derive(Clone, Copy)]
pub(super) enum LifecycleAction {
    Install,
    Upgrade,
    Uninstall,
}

pub(super) fn lifecycle(
    package: &Path,
    action: LifecycleAction,
) -> Result<Output, Box<dyn std::error::Error>> {
    let stem = match action {
        LifecycleAction::Install => "Install-Peritus",
        LifecycleAction::Upgrade => "Upgrade-Peritus",
        LifecycleAction::Uninstall => "Uninstall-Peritus",
    };
    let mut command = if cfg!(windows) {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]);
        command.arg(package.join(format!("{stem}.ps1")));
        if !matches!(action, LifecycleAction::Uninstall) {
            command.arg("-BundleRoot").arg(package);
        }
        command
    } else {
        let mut command = Command::new("sh");
        command.arg(package.join(format!("{stem}.sh")));
        if !matches!(action, LifecycleAction::Uninstall) {
            command.arg(package);
        }
        command
    };
    bounded_output(&mut command)
}

pub(super) fn command_output<I, S>(
    executable: &Path,
    arguments: I,
) -> Result<Output, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(executable);
    command.args(arguments);
    bounded_output(&mut command)
}

pub(super) fn require_success(
    output: &Output,
    action: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{action} failed with status {}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

pub(super) fn marker(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn bounded_output(command: &mut Command) -> Result<Output, Box<dyn std::error::Error>> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = command.output()?;
    if output.stdout.len().saturating_add(output.stderr.len()) > MAX_CHILD_OUTPUT {
        return Err("native H2 child output exceeded its controller bound".into());
    }
    Ok(output)
}
