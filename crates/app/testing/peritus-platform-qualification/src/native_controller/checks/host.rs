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
    let subject_root = package.parent().ok_or("native package has no subject root")?;
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
        command.arg("-InstallRoot").arg(subject_root.join("local-app-data/Programs/Peritus"));
        if matches!(action, LifecycleAction::Uninstall) {
            command.arg("-DataRoot").arg(subject_root.join("local-app-data/Peritus"));
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
    configure_subject_environment(&mut command, subject_root);
    bounded_output(&mut command)
}

fn configure_subject_environment(command: &mut Command, root: &Path) {
    command
        .env("HOME", root)
        .env("USERPROFILE", root)
        .env("LOCALAPPDATA", root.join("local-app-data"))
        .env("APPDATA", root.join("app-data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("TMPDIR", root.join("tmp"))
        .env("TEMP", root.join("tmp"))
        .env("TMP", root.join("tmp"));
    for name in ["PATH", "SYSTEMROOT", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
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

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;
    use std::process::Command;

    use super::configure_subject_environment;

    #[test]
    fn lifecycle_child_overrides_private_platform_directories_without_reclearing_the_controller() {
        let root = Path::new("qualification-root");
        let local_app_data = root.join("local-app-data");
        let temporary = root.join("tmp");
        let mut command = Command::new("unused");
        command.env("CONTROLLER_BOUND_VALUE", "retained");
        configure_subject_environment(&mut command, root);

        assert_eq!(configured(&command, "HOME"), Some(root.as_os_str()));
        assert_eq!(configured(&command, "LOCALAPPDATA"), Some(local_app_data.as_os_str()));
        assert_eq!(configured(&command, "TEMP"), Some(temporary.as_os_str()));
        assert_eq!(configured(&command, "CONTROLLER_BOUND_VALUE"), Some(OsStr::new("retained")));
    }

    fn configured<'a>(command: &'a Command, name: &str) -> Option<&'a OsStr> {
        command.get_envs().find_map(|(key, value)| (key == name).then_some(value).flatten())
    }
}
