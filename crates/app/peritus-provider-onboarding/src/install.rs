//! Explicitly approved provider installation through the vendors' native installers.

use std::{
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

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
    let mut child = command.spawn().map_err(|error| installation_error(kind, stage, &error))?;
    let deadline = Instant::now() + Duration::from_mins(15);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(installation_error(kind, stage, &error));
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(installation_error(
                kind,
                stage,
                &"installer exceeded the 15 minute execution deadline",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    };
    if status.success() { Ok(()) } else { Err(installation_error(kind, stage, &status)) }
}

/// Whether the official OpenCode executable can be pinned without opening a login session.
#[must_use]
pub fn opencode_runtime_installed() -> bool {
    crate::models::run(async {
        peritus_provider_opencode::OpenCodeExecutable::discover_supported(
            &peritus_provider_core::CancellationToken::new(),
        )
        .await
        .map(|_| ())
    })
    .is_ok()
}

/// Installs the official OpenCode 1.19.0 CLI after foreground download approval.
///
/// API credentials are supplied separately from Peritus's OS credential store. This never
/// starts a login flow or changes the user's existing OpenCode configuration.
///
/// # Errors
/// Returns download, installer, or executable-discovery failures.
pub fn install_opencode_runtime() -> Result<(), OnboardingError> {
    if opencode_runtime_installed() {
        return Ok(());
    }
    let kind = ProviderKind::OpenCodeZen;
    if cfg!(windows) {
        run_command(
            kind,
            "native installer",
            Command::new("npm.cmd").args(["install", "--global", "opencode-ai@1.19.0"]),
        )?;
    } else {
        let temporary = tempfile::Builder::new()
            .prefix("peritus-opencode-install-")
            .tempdir()
            .map_err(|error| installation_error(kind, "temporary directory creation", &error))?;
        let script = temporary.path().join("install.sh");
        let installer =
            Installer { url: "https://opencode.ai/install", interpreter: "bash", windows: false };
        run_command(kind, "download", &mut installer.download(&script))?;
        let metadata = std::fs::metadata(&script)
            .map_err(|error| installation_error(kind, "download inspection", &error))?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 1024 * 1024 {
            return Err(installation_error(
                kind,
                "download inspection",
                &"installer size is invalid",
            ));
        }
        run_command(
            kind,
            "native installer",
            installer.execute(&script).args(["--version", "1.19.0", "--no-modify-path"]),
        )?;
    }
    if opencode_runtime_installed() {
        Ok(())
    } else {
        Err(installation_error(
            kind,
            "native installer",
            &"supported OpenCode 1.19.0 was not found after installation",
        ))
    }
}

fn installation_error(
    kind: ProviderKind,
    stage: &'static str,
    detail: &dyn std::fmt::Display,
) -> OnboardingError {
    OnboardingError::Installation { provider: kind.label(), stage, detail: detail.to_string() }
}
