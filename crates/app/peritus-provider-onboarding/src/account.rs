//! Official executable discovery, status, and interactive login delegation.

use std::{
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};

use peritus_product_state::ProviderKind;
use peritus_provider_anthropic::ClaudeExecutable;
use peritus_provider_core::{CancellationToken, cancel_first};
use peritus_provider_openai::CodexExecutable;
use tokio::{
    io::{AsyncRead, AsyncReadExt as _},
    process::{Child, Command},
};
use zeroize::Zeroizing;

use crate::{OnboardingError, ProviderObservation, ProviderStatus};

const MAX_RETAINED_STATUS_BYTES: usize = 64 * 1024;
const STATUS_EVIDENCE_OVERLAP_BYTES: usize = 64;

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
    /// Pins an account executable from PATH or its standard native installation directory.
    ///
    /// # Errors
    ///
    /// Returns an unavailable error when the executable cannot be found and pinned.
    pub fn discover(kind: ProviderKind) -> Result<Self, OnboardingError> {
        let discovered = match kind {
            ProviderKind::CodexAccount => {
                CodexExecutable::discover().map(|value| value.as_path().to_owned())
            }
            ProviderKind::ClaudeAccount => {
                ClaudeExecutable::discover().map(|value| value.as_path().to_owned())
            }
            _ => return Err(OnboardingError::UnsupportedProvider),
        };
        let executable =
            discovered.ok().or_else(|| discover_native(kind)).ok_or_else(|| unavailable(kind))?;
        Ok(Self { kind, executable })
    }

    /// Observes current login state using cancellable, bounded in-memory process output.
    ///
    /// # Errors
    ///
    /// Returns a cancellation failure when the caller cancels the observation. Local process
    /// failures are represented by an infrastructure observation with a credential-safe cause.
    #[must_use]
    pub async fn status(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<ProviderObservation, OnboardingError> {
        let Some(command) = status_command(self.kind, &self.executable) else {
            return Err(OnboardingError::UnsupportedProvider);
        };
        let output = match run_status_command(command, cancellation).await {
            Ok(output) => output,
            Err(StatusCommandError::Cancelled) => return Err(OnboardingError::Cancelled),
            Err(error) => {
                return Ok(ProviderObservation::with_diagnostic(
                    self.kind,
                    ProviderStatus::Infrastructure,
                    Some(self.executable.clone()),
                    error.diagnostic(),
                ));
            }
        };
        let status = parse_observed_status(self.kind, output.status.success(), &output);
        let observed_bytes = output.stdout.total_bytes.saturating_add(output.stderr.total_bytes);
        let diagnostic = (status == ProviderStatus::Unknown).then(|| {
            unknown_status_diagnostic(
                output.status.success(),
                output.status.code(),
                observed_bytes,
            )
        });
        Ok(match diagnostic {
            Some(diagnostic) => ProviderObservation::with_diagnostic(
                self.kind,
                status,
                Some(self.executable.clone()),
                diagnostic,
            ),
            None => ProviderObservation::new(self.kind, status, Some(self.executable.clone())),
        })
    }

    /// Hands terminal ownership to the official interactive login and verifies its result.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe process or incomplete-login failure.
    pub async fn login(
        &self,
        mode: AccountLogin,
        cancellation: &CancellationToken,
    ) -> Result<ProviderObservation, OnboardingError> {
        if cancellation.is_cancelled() {
            return Err(OnboardingError::Cancelled);
        }
        let mut command = Command::new(&self.executable);
        preserve_terminal_title(&mut command, self.kind);
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
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| OnboardingError::LoginProcess {
                provider: self.kind.label(),
                detail: error.to_string(),
            })?;
        let status = match cancel_first(cancellation, child.wait()).await {
            Some(status) => status.map_err(|error| OnboardingError::LoginProcess {
                provider: self.kind.label(),
                detail: error.to_string(),
            })?,
            None => {
                terminate(&mut child).await;
                return Err(OnboardingError::Cancelled);
            }
        };
        if !status.success() {
            return Err(OnboardingError::LoginIncomplete { provider: self.kind.label() });
        }
        let observation = self.status(cancellation).await?;
        if observation.status() == ProviderStatus::SignedOut {
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

fn discover_native(kind: ProviderKind) -> Option<PathBuf> {
    let (variable, relative) = match (kind, cfg!(windows)) {
        (ProviderKind::CodexAccount, true) => ("LOCALAPPDATA", "Programs/OpenAI/Codex/bin"),
        (_, true) => ("USERPROFILE", ".local/bin"),
        (_, false) => ("HOME", ".local/bin"),
    };
    let base = PathBuf::from(std::env::var_os(variable)?);
    if !base.is_absolute() {
        return None;
    }
    pin_native_directory(kind, &base.join(relative))
}

fn pin_native_directory(kind: ProviderKind, directory: &Path) -> Option<PathBuf> {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    match kind {
        ProviderKind::CodexAccount => {
            CodexExecutable::pin(directory.join(format!("codex{suffix}")))
                .ok()
                .map(|value| value.as_path().to_owned())
        }
        ProviderKind::ClaudeAccount => {
            ClaudeExecutable::pin(directory.join(format!("claude{suffix}")))
                .ok()
                .map(|value| value.as_path().to_owned())
        }
        _ => None,
    }
}

/// Complete built-in account-provider catalog.
pub struct ProviderCatalog;

impl ProviderCatalog {
    /// Observes both official account routes in stable presentation order.
    ///
    /// # Errors
    ///
    /// Returns a cancellation failure when the caller cancels either independent probe.
    #[must_use]
    pub async fn observe(
        cancellation: &CancellationToken,
    ) -> Result<Vec<ProviderObservation>, OnboardingError> {
        let (codex, claude) = tokio::join!(
            observe_provider(ProviderKind::CodexAccount, cancellation),
            observe_provider(ProviderKind::ClaudeAccount, cancellation),
        );
        Ok(vec![codex?, claude?])
    }
}

async fn observe_provider(
    kind: ProviderKind,
    cancellation: &CancellationToken,
) -> Result<ProviderObservation, OnboardingError> {
    if cancellation.is_cancelled() {
        return Err(OnboardingError::Cancelled);
    }
    let discovery = tokio::task::spawn_blocking(move || AccountProvider::discover(kind));
    let provider = match cancel_first(cancellation, discovery).await {
        None => return Err(OnboardingError::Cancelled),
        Some(Ok(Ok(provider))) => provider,
        Some(Ok(Err(_))) => {
            return Ok(ProviderObservation::new(kind, ProviderStatus::Unavailable, None));
        }
        Some(Err(_)) => {
            return Ok(ProviderObservation::with_diagnostic(
                kind,
                ProviderStatus::Infrastructure,
                None,
                "provider executable discovery task failed".to_owned(),
            ));
        }
    };
    if cancellation.is_cancelled() {
        return Err(OnboardingError::Cancelled);
    }
    provider.status(cancellation).await
}

fn status_command(kind: ProviderKind, executable: &Path) -> Option<Command> {
    let mut command = Command::new(executable);
    preserve_terminal_title(&mut command, kind);
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
    command.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    {
        // Piped output alone does not prevent a child from changing the shared console title.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Some(command)
}

fn preserve_terminal_title(command: &mut Command, kind: ProviderKind) {
    if kind == ProviderKind::ClaudeAccount {
        command.env("CLAUDE_CODE_DISABLE_TERMINAL_TITLE", "1");
    }
}

fn parse_status(
    kind: ProviderKind,
    success: bool,
    stdout: &[u8],
    stderr: &[u8],
) -> ProviderStatus {
    match kind {
        ProviderKind::CodexAccount => {
            if contains_ascii_case_insensitive(stdout, b"not logged in")
                || contains_ascii_case_insensitive(stderr, b"not logged in")
            {
                ProviderStatus::SignedOut
            } else if success
                && (contains_ascii_case_insensitive(stdout, b"logged in")
                    || contains_ascii_case_insensitive(stderr, b"logged in"))
            {
                ProviderStatus::Ready
            } else {
                ProviderStatus::Unknown
            }
        }
        ProviderKind::ClaudeAccount => match (success, claude_login_state(stdout)) {
            (_, Some(false)) => ProviderStatus::SignedOut,
            (true, Some(true)) => ProviderStatus::Ready,
            _ => ProviderStatus::Unknown,
        },
        _ => ProviderStatus::Unknown,
    }
}

fn claude_login_state(stdout: &[u8]) -> Option<bool> {
    serde_json::from_slice::<serde_json::Value>(stdout).ok().and_then(|status| {
        status
            .as_object()?
            .get("loggedIn")
            .or_else(|| status.as_object()?.get("logged_in"))
            .and_then(serde_json::Value::as_bool)
    })
}

fn contains_ascii_case_insensitive(value: &[u8], needle: &[u8]) -> bool {
    value.windows(needle.len()).any(|candidate| candidate.eq_ignore_ascii_case(needle))
}

fn parse_observed_status(
    kind: ProviderKind,
    success: bool,
    output: &StatusCommandOutput,
) -> ProviderStatus {
    let retained = parse_status(
        kind,
        success,
        &output.stdout.retained,
        &output.stderr.retained,
    );
    if retained != ProviderStatus::Unknown {
        return retained;
    }
    match kind {
        ProviderKind::CodexAccount => {
            if output.stdout.evidence.codex_signed_out
                || output.stderr.evidence.codex_signed_out
            {
                ProviderStatus::SignedOut
            } else if success
                && (output.stdout.evidence.codex_ready || output.stderr.evidence.codex_ready)
            {
                ProviderStatus::Ready
            } else {
                ProviderStatus::Unknown
            }
        }
        ProviderKind::ClaudeAccount => {
            let logged_in = output
                .stdout
                .evidence
                .claude_login_state()
                .or_else(|| output.stderr.evidence.claude_login_state());
            match (success, logged_in) {
                (_, Some(false)) => ProviderStatus::SignedOut,
                (true, Some(true)) => ProviderStatus::Ready,
                _ => ProviderStatus::Unknown,
            }
        }
        _ => ProviderStatus::Unknown,
    }
}

fn unknown_status_diagnostic(success: bool, code: Option<i32>, observed_bytes: u64) -> String {
    if success {
        return format!(
            "status command returned an unrecognized response after {observed_bytes} output bytes"
        );
    }
    code.map_or_else(
        || {
            format!(
                "status command exited unsuccessfully without a numeric code after {observed_bytes} output bytes"
            )
        },
        |code| {
            format!(
                "status command exited with code {code} after {observed_bytes} output bytes"
            )
        },
    )
}

struct StatusCommandOutput {
    status: ExitStatus,
    stdout: StatusStreamOutput,
    stderr: StatusStreamOutput,
}

struct StatusStreamOutput {
    retained: Zeroizing<Vec<u8>>,
    total_bytes: u64,
    evidence: StatusEvidence,
}

struct StatusEvidence {
    codex_ready: bool,
    codex_signed_out: bool,
    claude_logged_in: bool,
    claude_logged_out: bool,
    raw_tail: Zeroizing<Vec<u8>>,
    json_tail: Zeroizing<Vec<u8>>,
}

impl StatusEvidence {
    fn new() -> Self {
        Self {
            codex_ready: false,
            codex_signed_out: false,
            claude_logged_in: false,
            claude_logged_out: false,
            raw_tail: Zeroizing::new(Vec::new()),
            json_tail: Zeroizing::new(Vec::new()),
        }
    }

    fn observe(&mut self, bytes: &[u8]) {
        let mut raw = Zeroizing::new(Vec::with_capacity(self.raw_tail.len() + bytes.len()));
        raw.extend_from_slice(&self.raw_tail);
        raw.extend_from_slice(bytes);
        self.codex_signed_out |= contains_ascii_case_insensitive(&raw, b"not logged in");
        self.codex_ready |= contains_ascii_case_insensitive(&raw, b"logged in");
        retain_tail(&mut self.raw_tail, &raw);

        let mut json = Zeroizing::new(Vec::with_capacity(self.json_tail.len() + bytes.len()));
        json.extend_from_slice(&self.json_tail);
        json.extend(bytes.iter().copied().filter(|byte| !byte.is_ascii_whitespace()));
        self.claude_logged_in |= contains_exact(&json, br#""loggedIn":true"#)
            || contains_exact(&json, br#""logged_in":true"#);
        self.claude_logged_out |= contains_exact(&json, br#""loggedIn":false"#)
            || contains_exact(&json, br#""logged_in":false"#);
        retain_tail(&mut self.json_tail, &json);
    }

    const fn claude_login_state(&self) -> Option<bool> {
        match (self.claude_logged_in, self.claude_logged_out) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        }
    }
}

fn retain_tail(target: &mut Zeroizing<Vec<u8>>, source: &[u8]) {
    let start = source.len().saturating_sub(STATUS_EVIDENCE_OVERLAP_BYTES);
    target.clear();
    target.extend_from_slice(&source[start..]);
}

fn contains_exact(value: &[u8], needle: &[u8]) -> bool {
    value.windows(needle.len()).any(|candidate| candidate == needle)
}

enum StatusCommandError {
    Cancelled,
    Spawn(std::io::ErrorKind),
    MissingPipe(&'static str),
    Read(&'static str, std::io::ErrorKind),
    Wait(std::io::ErrorKind),
}

impl StatusCommandError {
    fn diagnostic(&self) -> String {
        match self {
            Self::Cancelled => "status observation was cancelled".to_owned(),
            Self::Spawn(kind) => {
                format!("status process could not be started ({kind:?})")
            }
            Self::MissingPipe(stream) => {
                format!("status process did not provide its {stream} pipe")
            }
            Self::Read(stream, kind) => {
                format!("status process {stream} could not be read ({kind:?})")
            }
            Self::Wait(kind) => {
                format!("status process terminal state could not be read ({kind:?})")
            }
        }
    }
}

async fn run_status_command(
    mut command: Command,
    cancellation: &CancellationToken,
) -> Result<StatusCommandOutput, StatusCommandError> {
    if cancellation.is_cancelled() {
        return Err(StatusCommandError::Cancelled);
    }
    let mut child = command.spawn().map_err(|error| StatusCommandError::Spawn(error.kind()))?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate(&mut child).await;
            return Err(StatusCommandError::MissingPipe("stdout"));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate(&mut child).await;
            return Err(StatusCommandError::MissingPipe("stderr"));
        }
    };
    let operation = async {
        let (stdout, stderr, status) = tokio::try_join!(
            read_status_stream(stdout, "stdout"),
            read_status_stream(stderr, "stderr"),
            async {
                child
                    .wait()
                    .await
                    .map_err(|error| StatusCommandError::Wait(error.kind()))
            },
        )?;
        Ok(StatusCommandOutput { status, stdout, stderr })
    };
    match cancel_first(cancellation, operation).await {
        Some(Ok(output)) => Ok(output),
        Some(Err(error)) => {
            terminate(&mut child).await;
            Err(error)
        }
        None => {
            terminate(&mut child).await;
            Err(StatusCommandError::Cancelled)
        }
    }
}

async fn read_status_stream(
    mut reader: impl AsyncRead + Unpin,
    stream: &'static str,
) -> Result<StatusStreamOutput, StatusCommandError> {
    let mut retained = Zeroizing::new(Vec::new());
    let mut total_bytes = 0_u64;
    let mut evidence = StatusEvidence::new();
    let mut chunk = Zeroizing::new([0_u8; 8 * 1024]);
    loop {
        let count = reader
            .read(&mut chunk[..])
            .await
            .map_err(|error| StatusCommandError::Read(stream, error.kind()))?;
        if count == 0 {
            return Ok(StatusStreamOutput { retained, total_bytes, evidence });
        }
        total_bytes = total_bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        evidence.observe(&chunk[..count]);
        let remaining = MAX_RETAINED_STATUS_BYTES.saturating_sub(retained.len());
        if remaining != 0 {
            retained.extend_from_slice(&chunk[..count.min(remaining)]);
        }
    }
}

async fn terminate(child: &mut Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

const fn unavailable(kind: ProviderKind) -> OnboardingError {
    OnboardingError::ExecutableUnavailable { provider: kind.label() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_directory_pins_an_installed_tool_without_changing_path() {
        let temporary = tempfile::tempdir().expect("fixture directory");
        let name = if cfg!(windows) { "codex.exe" } else { "codex" };
        let path = temporary.path().join(name);
        std::fs::write(&path, "fixture executable").expect("fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("executable");
        }
        assert_eq!(
            pin_native_directory(ProviderKind::CodexAccount, temporary.path()),
            Some(path.canonicalize().expect("canonical path"))
        );
        assert!(pin_native_directory(ProviderKind::ClaudeAccount, temporary.path()).is_none());
        assert!(pin_native_directory(ProviderKind::OpenAiApi, temporary.path()).is_none());
    }

    #[test]
    fn parsers_retain_only_login_state() {
        assert_eq!(
            parse_status(ProviderKind::CodexAccount, true, b"Logged in using ChatGPT", b""),
            ProviderStatus::Ready
        );
        assert_eq!(
            parse_status(
                ProviderKind::ClaudeAccount,
                true,
                br#"{"loggedIn":true,"email":"must-not-escape@example.invalid"}"#,
                b"",
            ),
            ProviderStatus::Ready
        );
        assert_eq!(
            parse_status(ProviderKind::ClaudeAccount, false, br#"{"loggedIn":false}"#, b""),
            ProviderStatus::SignedOut
        );
        assert_eq!(
            parse_status(ProviderKind::ClaudeAccount, false, b"private diagnostic", b""),
            ProviderStatus::Unknown
        );
    }
}

#[cfg(all(test, windows))]
mod windows_tests;
