//! Official executable discovery, status, and interactive login delegation.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use peritus_product_state::ProviderKind;
use peritus_provider_anthropic::ClaudeExecutable;
use peritus_provider_openai::CodexExecutable;

use crate::{OnboardingError, ProviderObservation, ProviderStatus};

const MAX_STATUS_BYTES: usize = 64 * 1024;

/// Supported account login presentation for the Codex route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountLogin {
    /// Let the official client use its ordinary browser-oriented login.
    Browser,
    /// Ask Codex to use its device-code flow with a textual fallback.
    Device,
}

/// One pinned credential-owning account provider executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountProvider {
    kind: ProviderKind,
    executable: PathBuf,
}

impl AccountProvider {
    /// Discovers and pins the official executable for one account route.
    ///
    /// # Errors
    ///
    /// Returns an unavailable error when the executable cannot be found and pinned.
    pub fn discover(kind: ProviderKind) -> Result<Self, OnboardingError> {
        let executable = match kind {
            ProviderKind::CodexAccount => CodexExecutable::discover()
                .map(|value| value.as_path().to_owned())
                .map_err(|_| unavailable(kind))?,
            ProviderKind::ClaudeAccount => ClaudeExecutable::discover()
                .map(|value| value.as_path().to_owned())
                .map_err(|_| unavailable(kind))?,
            _ => return Err(OnboardingError::UnsupportedProvider),
        };
        Ok(Self { kind, executable })
    }

    /// Observes current login state without returning or retaining command output.
    #[must_use]
    pub fn status(&self) -> ProviderObservation {
        let Some(mut command) = status_command(self.kind, &self.executable) else {
            return ProviderObservation::new(
                self.kind,
                ProviderStatus::NeedsAttention,
                Some(self.executable.clone()),
            );
        };
        let Ok(output) = command.output() else {
            return ProviderObservation::new(
                self.kind,
                ProviderStatus::NeedsAttention,
                Some(self.executable.clone()),
            );
        };
        let status =
            if output.stdout.len() > MAX_STATUS_BYTES || output.stderr.len() > MAX_STATUS_BYTES {
                ProviderStatus::NeedsAttention
            } else {
                parse_status(self.kind, output.status.success(), &output.stdout)
            };
        ProviderObservation::new(self.kind, status, Some(self.executable.clone()))
    }

    /// Hands terminal ownership to the official interactive login and verifies its result.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe process or incomplete-login failure.
    pub fn login(&self, mode: AccountLogin) -> Result<ProviderObservation, OnboardingError> {
        let mut command = Command::new(&self.executable);
        match self.kind {
            ProviderKind::CodexAccount => {
                command.arg("login");
                if mode == AccountLogin::Device {
                    command.arg("--device-auth");
                }
            }
            ProviderKind::ClaudeAccount => {
                command.args(["auth", "login"]);
            }
            _ => return Err(OnboardingError::UnsupportedProvider),
        }
        let status = command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|error| OnboardingError::LoginProcess {
                provider: self.kind.label(),
                detail: error.to_string(),
            })?;
        if !status.success() {
            return Err(OnboardingError::LoginIncomplete { provider: self.kind.label() });
        }
        let observation = self.status();
        if observation.status() != ProviderStatus::Ready {
            return Err(OnboardingError::LoginIncomplete { provider: self.kind.label() });
        }
        Ok(observation)
    }

    /// Returns the account route kind.
    #[must_use]
    pub const fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Borrows the pinned executable path.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

/// Complete built-in account-provider catalog.
pub struct ProviderCatalog;

impl ProviderCatalog {
    /// Observes both official account routes in stable presentation order.
    #[must_use]
    pub fn observe() -> Vec<ProviderObservation> {
        [ProviderKind::CodexAccount, ProviderKind::ClaudeAccount]
            .into_iter()
            .map(|kind| {
                AccountProvider::discover(kind).map_or_else(
                    |_| ProviderObservation::new(kind, ProviderStatus::Unavailable, None),
                    |provider| provider.status(),
                )
            })
            .collect()
    }
}

fn status_command(kind: ProviderKind, executable: &Path) -> Option<Command> {
    let mut command = Command::new(executable);
    match kind {
        ProviderKind::CodexAccount => {
            command.args(["login", "status"]);
        }
        ProviderKind::ClaudeAccount => {
            command.args(["auth", "status", "--json"]);
        }
        _ => return None,
    }
    command.stdin(Stdio::null());
    Some(command)
}

fn parse_status(kind: ProviderKind, success: bool, stdout: &[u8]) -> ProviderStatus {
    if !success {
        return ProviderStatus::SignedOut;
    }
    match kind {
        ProviderKind::CodexAccount => ProviderStatus::Ready,
        ProviderKind::ClaudeAccount => serde_json::from_slice::<serde_json::Value>(stdout)
            .ok()
            .and_then(|status| {
                status
                    .as_object()?
                    .get("loggedIn")
                    .or_else(|| status.as_object()?.get("logged_in"))
                    .and_then(serde_json::Value::as_bool)
            })
            .map_or(ProviderStatus::NeedsAttention, |logged_in| {
                if logged_in { ProviderStatus::Ready } else { ProviderStatus::SignedOut }
            }),
        _ => ProviderStatus::NeedsAttention,
    }
}

const fn unavailable(kind: ProviderKind) -> OnboardingError {
    OnboardingError::ExecutableUnavailable { provider: kind.label() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers_retain_only_login_state() {
        assert_eq!(
            parse_status(ProviderKind::CodexAccount, true, b"Logged in using ChatGPT"),
            ProviderStatus::Ready
        );
        assert_eq!(
            parse_status(
                ProviderKind::ClaudeAccount,
                true,
                br#"{"loggedIn":true,"email":"must-not-escape@example.invalid"}"#,
            ),
            ProviderStatus::Ready
        );
        assert_eq!(
            parse_status(ProviderKind::ClaudeAccount, false, b"private diagnostic"),
            ProviderStatus::SignedOut
        );
    }
}
