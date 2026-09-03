//! Authentication delegation and one-turn Claude executable ownership.

mod result;

use core::fmt;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use peritus_model_protocol::{
    FailureCategory, ModelFailure, ModelRequest, OutcomeCertainty, ProviderName,
    RedactedDiagnostic, Retryability, TransportPhase,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, EnvironmentName, ModelProvider, OwnedModelStream, ProcessLimits,
    ProcessRequest, ProcessTransport, ProviderAvailability, ProviderCoreError,
    ProviderCoreErrorKind, TokioProcessTransport, validate_request_profile,
};

use super::config::ClaudeRuntimeConfig;
use super::request::RuntimeRequest;
use super::stream::ClaudeRuntimeStream;

const EMPTY_MCP_CONFIG: &str = r#"{"mcpServers":{}}"#;

/// Account-backed Anthropic provider constrained to credential-owned model routing.
pub struct ClaudeRuntimeProvider {
    config: ClaudeRuntimeConfig,
    transport: Box<dyn ProcessTransport>,
    authenticated: AtomicBool,
}

impl ClaudeRuntimeProvider {
    /// Creates a production provider backed by the pinned official executable.
    #[must_use]
    pub fn new(config: ClaudeRuntimeConfig) -> Self {
        Self {
            config,
            transport: Box::new(TokioProcessTransport),
            authenticated: AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_transport(
        config: ClaudeRuntimeConfig,
        transport: Box<dyn ProcessTransport>,
    ) -> Self {
        Self { config, transport, authenticated: AtomicBool::new(false) }
    }

    /// Returns the exact immutable runtime profile.
    #[must_use]
    pub const fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        self.config.profile()
    }

    /// Delegates login-state inspection to `claude auth status --json`.
    ///
    /// Peritus neither starts login nor reads Claude's credential storage. An unauthenticated
    /// result instructs the user to run `claude auth login` externally.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe process, cancellation, malformed-status, or authentication error.
    #[must_use]
    pub fn require_authenticated<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async move {
            let result = self.check_authenticated(cancellation).await;
            self.authenticated.store(result.is_ok(), Ordering::Release);
            result
        })
    }

    async fn check_authenticated(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<(), ProviderCoreError> {
        let request = ProcessRequest::new(
            self.config.executable().process_executable().clone(),
            vec!["auth".to_owned(), "status".to_owned(), "--json".to_owned()],
            Vec::new(),
            None,
            credential_environment()?,
            auth_limits()?,
        )?;
        let output = self.transport.run(request, cancellation).await?;
        if !output.exit().success() {
            return Err(not_authenticated());
        }
        let status: serde_json::Value =
            serde_json::from_slice(output.stdout()).map_err(|_| not_authenticated())?;
        let logged_in = status
            .as_object()
            .and_then(|status| status.get("loggedIn").or_else(|| status.get("logged_in")))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if logged_in { Ok(()) } else { Err(not_authenticated()) }
    }

    fn start_inner(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            validate_request_profile(self.config.profile(), &request)?;
            let runtime = super::request::encode(&request)?;
            if let Err(error) = self.require_authenticated(&cancellation).await {
                let stream = if error.kind() == ProviderCoreErrorKind::Cancelled {
                    ClaudeRuntimeStream::cancelled(request.model().clone())?
                } else if error.kind() == ProviderCoreErrorKind::InvalidCredential {
                    ClaudeRuntimeStream::failed(
                        request.model().clone(),
                        authentication_failure(self.config.profile().provider().clone())?,
                        b"claude-runtime-authentication",
                    )?
                } else {
                    return Err(error);
                };
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
            let output = match self.run_turn(&request, &runtime, &cancellation).await {
                Ok(output) => output,
                Err(error) if error.kind() == ProviderCoreErrorKind::Cancelled => {
                    let stream = ClaudeRuntimeStream::cancelled(request.model().clone())?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
                Err(error) if error.kind() == ProviderCoreErrorKind::Connect => return Err(error),
                Err(error) => {
                    let category = if error.operation() == "process_timeout" {
                        FailureCategory::Timeout
                    } else {
                        FailureCategory::AmbiguousAcceptance
                    };
                    let stream = ClaudeRuntimeStream::failed(
                        request.model().clone(),
                        runtime_failure(
                            self.config.profile().provider().clone(),
                            category,
                            OutcomeCertainty::MaybeAccepted,
                            "anthropic.claude_runtime.transport",
                        )?,
                        b"claude-runtime-transport",
                    )?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
            };
            result::normalize(
                &request,
                &runtime,
                &output,
                self.config.profile().provider().clone(),
                cancellation,
            )
        })
    }

    async fn run_turn(
        &self,
        request: &ModelRequest,
        runtime: &RuntimeRequest,
        cancellation: &CancellationToken,
    ) -> Result<peritus_provider_core::ProcessOutput, ProviderCoreError> {
        let directory = tempfile::tempdir().map_err(|_| temporary_failure())?;
        let mut system =
            tempfile::NamedTempFile::new_in(directory.path()).map_err(|_| temporary_failure())?;
        system.write_all(&runtime.system).map_err(|_| temporary_failure())?;
        system.flush().map_err(|_| temporary_failure())?;
        let system_path = path_argument(system.path())?;
        let arguments = vec![
            "-p".to_owned(),
            "--output-format".to_owned(),
            "json".to_owned(),
            "--model".to_owned(),
            request.model().as_str().to_owned(),
            "--effort".to_owned(),
            runtime.effort.to_owned(),
            "--safe-mode".to_owned(),
            "--tools".to_owned(),
            String::new(),
            "--disallowedTools".to_owned(),
            "mcp__*".to_owned(),
            "--disable-slash-commands".to_owned(),
            "--no-chrome".to_owned(),
            "--no-session-persistence".to_owned(),
            "--strict-mcp-config".to_owned(),
            "--mcp-config".to_owned(),
            EMPTY_MCP_CONFIG.to_owned(),
            "--system-prompt-file".to_owned(),
            system_path,
            "--json-schema".to_owned(),
            runtime.schema.clone(),
        ];
        let process = ProcessRequest::new(
            self.config.executable().process_executable().clone(),
            arguments,
            runtime.prompt.clone(),
            Some(directory.path().to_path_buf()),
            credential_environment()?,
            self.config.process_limits(),
        )?;
        self.transport.run(process, cancellation).await
    }
}

impl ModelProvider for ClaudeRuntimeProvider {
    fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        self.profile()
    }

    fn availability(&self) -> ProviderAvailability {
        if self.authenticated.load(Ordering::Acquire) {
            ProviderAvailability::CredentialPresent
        } else {
            ProviderAvailability::Unchecked
        }
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.start_inner(request, cancellation)
    }
}

impl fmt::Debug for ClaudeRuntimeProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeRuntimeProvider")
            .field("config", &self.config)
            .field("authenticated", &self.authenticated.load(Ordering::Acquire))
            .field("transport", &"[private owned process transport]")
            .finish()
    }
}

fn credential_environment() -> Result<Vec<EnvironmentName>, ProviderCoreError> {
    ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN"]
        .into_iter()
        .map(|name| EnvironmentName::new(name.to_owned()))
        .collect()
}

const fn auth_limits() -> Result<ProcessLimits, ProviderCoreError> {
    ProcessLimits::new(1, 64 * 1024, 64 * 1024, Duration::from_secs(10))
}

fn path_argument(path: &Path) -> Result<String, ProviderCoreError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        ProviderCoreError::configuration(
            "claude_runtime_tempfile",
            "temporary system-prompt path is not valid UTF-8",
        )
    })
}

const fn temporary_failure() -> ProviderCoreError {
    ProviderCoreError::configuration(
        "claude_runtime_tempfile",
        "private temporary runtime state could not be created",
    )
}

const fn not_authenticated() -> ProviderCoreError {
    ProviderCoreError::credential(
        "Claude is not authenticated; run `claude auth login` then `claude auth status`",
    )
}

fn authentication_failure(provider: ProviderName) -> Result<ModelFailure, ProviderCoreError> {
    failure(
        provider,
        FailureCategory::Authentication,
        TransportPhase::BeforeSend,
        OutcomeCertainty::DefinitelyNotAccepted,
        "anthropic.claude_runtime.authentication",
    )
}

fn runtime_failure(
    provider: ProviderName,
    category: FailureCategory,
    certainty: OutcomeCertainty,
    code: &'static str,
) -> Result<ModelFailure, ProviderCoreError> {
    let phase = match certainty {
        OutcomeCertainty::Terminal => TransportPhase::Completed,
        OutcomeCertainty::AcceptedPartial => TransportPhase::ReadingBody,
        OutcomeCertainty::DefinitelyNotAccepted | OutcomeCertainty::MaybeAccepted => {
            TransportPhase::SendingBody
        }
    };
    failure(provider, category, phase, certainty, code)
}

fn failure(
    provider: ProviderName,
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    code: &'static str,
) -> Result<ModelFailure, ProviderCoreError> {
    let diagnostic = RedactedDiagnostic::new(code.to_owned(), None, None, None).map_err(|_| {
        ProviderCoreError::configuration(
            "claude_runtime_failure",
            "static Claude runtime diagnostic could not be constructed",
        )
    })?;
    Ok(ModelFailure::new(
        provider,
        category,
        phase,
        certainty,
        if certainty == OutcomeCertainty::MaybeAccepted {
            Retryability::CallerDecision
        } else {
            Retryability::Never
        },
        None,
        None,
        None,
        diagnostic,
    ))
}
