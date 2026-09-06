//! Explicitly approved provider installation through the vendors' native installers.

use std::{path::Path, process::Command};

use peritus_product_state::ProviderKind;

use crate::{AccountProvider, OnboardingError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Installer {
    url: &'static str,
    interpreter: &'static str,
    windows: bool,
}

impl Installer {
    const fn for_provider(kind: ProviderKind, windows: bool) -> Result<Self, OnboardingError> {
        let (url, interpreter) = match (kind, windows) {
            (ProviderKind::CodexAccount, false) => ("https://chatgpt.com/codex/install.sh", "sh"),
            (ProviderKind::CodexAccount, true) => {
                ("https://chatgpt.com/codex/install.ps1", "powershell.exe")
            }
            (ProviderKind::ClaudeAccount, false) => ("https://claude.ai/install.sh", "bash"),
            (ProviderKind::ClaudeAccount, true) => {
                ("https://claude.ai/install.ps1", "powershell.exe")
            }
            _ => return Err(OnboardingError::UnsupportedProvider),
        };
        Ok(Self { url, interpreter, windows })
    }

    fn download(self, target: &Path) -> Command {
        let mut command = Command::new(if self.windows { "curl.exe" } else { "curl" });
        command
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--connect-timeout",
                "15",
                "--max-time",
                "120",
                "--max-filesize",
                "1048576",
                "--output",
            ])
            .arg(target)
            .arg(self.url);
        command
    }

    fn execute(self, script: &Path) -> Command {
        let mut command = Command::new(self.interpreter);
        if self.windows {
            command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        }
        command.arg(script);
        command
    }
}

/// Installs one official account tool after the user has explicitly approved the download.
///
/// This function does not obtain credentials or sign in. The vendor's installer owns its
/// native binary, dependencies, and installation policy. Terminal ownership stays with the
/// foreground process while that installer runs. Callers must request approval first.
///
/// # Errors
///
/// Returns an unsupported-route, download, installer, or installed-executable failure.
pub fn install_account_provider(kind: ProviderKind) -> Result<AccountProvider, OnboardingError> {
    let installer = Installer::for_provider(kind, cfg!(windows))?;
    if let Ok(provider) = AccountProvider::discover(kind) {
        return Ok(provider);
    }
    let temporary = tempfile::Builder::new()
        .prefix("peritus-provider-install-")
        .tempdir()
        .map_err(|error| installation_error(kind, "temporary directory creation", &error))?;
    let script =
        temporary.path().join(if installer.windows { "install.ps1" } else { "install.sh" });
    run_command(kind, "download", &mut installer.download(&script))?;
    let metadata = std::fs::metadata(&script)
        .map_err(|error| installation_error(kind, "download inspection", &error))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 1024 * 1024 {
        return Err(installation_error(kind, "download inspection", &"installer size is invalid"));
    }
    run_command(kind, "native installer", &mut installer.execute(&script))?;
    AccountProvider::discover(kind)
}

fn run_command(
    kind: ProviderKind,
    stage: &'static str,
    command: &mut Command,
) -> Result<(), OnboardingError> {
    let status = command.status().map_err(|error| installation_error(kind, stage, &error))?;
    if status.success() { Ok(()) } else { Err(installation_error(kind, stage, &status)) }
}

fn installation_error(
    kind: ProviderKind,
    stage: &'static str,
    detail: &dyn std::fmt::Display,
) -> OnboardingError {
    OnboardingError::Installation { provider: kind.label(), stage, detail: detail.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_account_platform_uses_the_official_native_installer() {
        for (kind, windows, url, interpreter) in [
            (ProviderKind::CodexAccount, false, "https://chatgpt.com/codex/install.sh", "sh"),
            (
                ProviderKind::CodexAccount,
                true,
                "https://chatgpt.com/codex/install.ps1",
                "powershell.exe",
            ),
            (ProviderKind::ClaudeAccount, false, "https://claude.ai/install.sh", "bash"),
            (ProviderKind::ClaudeAccount, true, "https://claude.ai/install.ps1", "powershell.exe"),
        ] {
            let installer = Installer::for_provider(kind, windows).expect("account installer");
            assert_eq!(installer.url, url);
            assert_eq!(installer.interpreter, interpreter);
            let path = Path::new("path with spaces/install-script");
            let download = installer.download(path);
            let args = download
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(args.windows(2).any(|pair| pair == ["--proto", "=https"]));
            assert!(args.windows(2).any(|pair| pair == ["--max-time", "120"]));
            assert!(args.windows(2).any(|pair| pair == ["--max-filesize", "1048576"]));
            assert_eq!(args.last().expect("URL"), url);
            let execution = installer.execute(path);
            assert_eq!(execution.get_program(), interpreter);
            assert_eq!(execution.get_args().last().expect("script argument"), path.as_os_str());
        }
    }

    #[test]
    fn direct_api_routes_never_have_an_executable_installer() {
        for kind in [
            ProviderKind::OpenAiApi,
            ProviderKind::AnthropicApi,
            ProviderKind::GoogleGeminiApi,
            ProviderKind::CompatibleEndpoint,
        ] {
            for windows in [false, true] {
                assert!(matches!(
                    Installer::for_provider(kind, windows),
                    Err(OnboardingError::UnsupportedProvider)
                ));
            }
        }
    }

    #[test]
    fn command_failure_is_not_reported_as_installation_success() {
        let mut command = Command::new("peritus-missing-installer-fixture-command");
        assert!(matches!(
            run_command(ProviderKind::CodexAccount, "download", &mut command),
            Err(OnboardingError::Installation { stage: "download", .. })
        ));
    }
}
