//! One-turn ownership of the pre-authenticated Codex executable.

mod cached;
mod failure;
mod invocation;

use core::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

use peritus_model_protocol::{
    FailureCategory, ModelRequest, OutcomeCertainty, Retryability, TransportPhase,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProcessTransport,
    ProviderAvailability, ProviderCoreError, ProviderCoreErrorKind, TokioProcessTransport,
    validate_request_profile,
};

use super::CodexRuntimeConfig;
use super::output::{DecodeFailure, decode_with_usage};
use super::stream::CodexRuntimeStream;
use failure::{decode_failure, failure};

/// Account-backed `OpenAI` provider constrained to the credential-owning `Codex` executable.
pub struct CodexRuntimeProvider {
    config: CodexRuntimeConfig,
    transport: std::sync::Arc<dyn ProcessTransport>,
    authenticated: AtomicBool,
    working_directory: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<tempfile::TempDir>>>>,
    cache_anchor: std::sync::Arc<std::sync::OnceLock<Option<invocation::CacheAnchor>>>,
}

/// Conventional client name for the account-backed Codex runtime provider.
pub type CodexRuntimeClient = CodexRuntimeProvider;

impl CodexRuntimeProvider {
    /// Creates a production provider backed by the pinned official executable.
    #[must_use]
    pub fn new(config: CodexRuntimeConfig) -> Self {
        Self {
            config,
            transport: std::sync::Arc::new(TokioProcessTransport),
            authenticated: AtomicBool::new(false),
            working_directory: std::sync::Arc::new(std::sync::Mutex::new(None)),
            cache_anchor: std::sync::Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Returns the exact immutable runtime profile.
    #[must_use]
    pub const fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        self.config.profile()
    }

    fn working_directory(&self) -> Result<std::sync::Arc<tempfile::TempDir>, ProviderCoreError> {
        let directory = self.working_directory.lock().map_err(|_| {
            ProviderCoreError::configuration(
                "codex_runtime_directory",
                "runtime directory lock poisoned",
            )
        })?;
        if let Some(directory) = directory.as_ref() {
            return Ok(directory.clone());
        }
        drop(directory);
        let created = std::sync::Arc::new(private_runtime_directory().map_err(|_| {
            ProviderCoreError::configuration(
                "codex_runtime_directory",
                "private runtime directory unavailable",
            )
        })?);
        let mut directory = self.working_directory.lock().map_err(|_| {
            ProviderCoreError::configuration(
                "codex_runtime_directory",
                "runtime directory lock poisoned",
            )
        })?;
        Ok(directory.get_or_insert(created).clone())
    }

    /// Delegates login-state inspection to `codex login status`.
    ///
    /// Peritus neither starts login nor reads Codex credential storage. An unauthenticated result
    /// instructs the user to run `codex login` externally.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe process, cancellation, or authentication error.
    #[must_use]
    pub fn require_authenticated<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async move {
            let result = invocation::require_authenticated(
                &self.config,
                self.transport.as_ref(),
                cancellation,
            )
            .await;
            self.authenticated.store(result.is_ok(), Ordering::Release);
            result
        })
    }

    fn start_inner(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            validate_request_profile(self.config.profile(), &request)?;
            let runtime = super::request::encode(&request)?;
            let projection = runtime.projection.clone();
            if runtime.prompt.iter().filter(|byte| (**byte & 0xc0) != 0x80).count()
                > super::request::MAX_INPUT_CHARACTERS
            {
                let stream = decode_failure(
                    request.model().clone(),
                    self.config.profile().provider().clone(),
                    &DecodeFailure::InputLimit,
                    peritus_model_protocol::UsageCounters::default(),
                )?;
                return Ok(OwnedModelStream::new(stream, cancellation).with_projection(projection));
            }
            if let Err(error) = if self.authenticated.load(Ordering::Acquire) {
                Ok(())
            } else {
                self.require_authenticated(&cancellation).await
            } {
                let stream = if error.kind() == ProviderCoreErrorKind::Cancelled {
                    CodexRuntimeStream::cancelled(request.model().clone())?
                } else if error.kind() == ProviderCoreErrorKind::InvalidCredential {
                    CodexRuntimeStream::failed(
                        request.model().clone(),
                        failure::authentication(self.config.profile().provider().clone())?,
                        b"openai-codex-runtime-authentication",
                        false,
                    )?
                } else {
                    return Err(error);
                };
                return Ok(OwnedModelStream::new(stream, cancellation).with_projection(projection));
            }
            let model = request.model().clone();
            let progress = std::sync::Arc::new(super::live::Progress::default());
            let runtime_provider = Self {
                config: self.config.clone(),
                transport: self.transport.clone(),
                authenticated: AtomicBool::new(true),
                working_directory: self.working_directory.clone(),
                cache_anchor: self.cache_anchor.clone(),
            };
            let owned_cancellation = cancellation.clone();
            let observations = progress.clone();
            let operation = Box::pin(async move {
                runtime_provider
                    .finish_inner(request, runtime, owned_cancellation, observations)
                    .await
            });
            Ok(OwnedModelStream::new(
                super::live::LiveRuntimeStream::new(model, operation, progress),
                cancellation,
            )
            .with_projection(projection))
        })
    }

    async fn finish_inner(
        &self,
        request: ModelRequest,
        runtime: super::request::RuntimeRequest,
        cancellation: CancellationToken,
        progress: std::sync::Arc<super::live::Progress>,
    ) -> Result<OwnedModelStream, ProviderCoreError> {
        let working_directory = self.working_directory()?;
        let cache_anchor = self
            .ready_cache_anchor(&request, &runtime, working_directory.path(), &cancellation)
            .await;
        let output = match invocation::run_turn(
            invocation::TurnContext {
                config: &self.config,
                transport: self.transport.as_ref(),
                working_directory: working_directory.path(),
                cache_anchor,
                cancellation: &cancellation,
                progress,
            },
            &request,
            &runtime,
        )
        .await
        {
            Ok(output) => output,
            Err(error) if error.kind() == ProviderCoreErrorKind::Cancelled => {
                let stream = CodexRuntimeStream::cancelled(request.model().clone())?;
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
            Err(error) if error.kind() == ProviderCoreErrorKind::Connect => return Err(error),
            Err(error) => {
                let category = if error.operation() == "process_timeout" {
                    FailureCategory::Timeout
                } else {
                    FailureCategory::AmbiguousAcceptance
                };
                let stream = CodexRuntimeStream::failed(
                    request.model().clone(),
                    failure(
                        self.config.profile().provider().clone(),
                        category,
                        TransportPhase::SendingBody,
                        OutcomeCertainty::MaybeAccepted,
                        Retryability::CallerDecision,
                        "openai.codex_runtime.transport",
                        None,
                    )?,
                    b"openai-codex-runtime-transport",
                    true,
                )?;
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
        };
        let decoded = decode_with_usage(
            output.process.stdout(),
            &runtime.allowed_tools,
            runtime.min_calls..=runtime.max_calls,
            output.final_message.as_deref().ok(),
        );
        if !output.process.exit().success() {
            let decoded_turn = super::output::reported_process_failure(output.process.stderr())
                .map_or(decoded.turn, Err);
            return self.failed_process(
                &request,
                &decoded_turn,
                decoded.usage,
                !output.process.stdout().is_empty(),
                cancellation,
            );
        }
        let turn = match decoded.turn.and_then(|turn| output.final_message.map(|_| turn)) {
            Ok(turn) => turn,
            Err(reason) => {
                let stream = decode_failure(
                    request.model().clone(),
                    self.config.profile().provider().clone(),
                    &reason,
                    decoded.usage,
                )?;
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
        };
        let stream = CodexRuntimeStream::completed(&request, turn, output.process.stdout())?;
        Ok(OwnedModelStream::new(stream, cancellation))
    }

    async fn ready_cache_anchor<'a>(
        &'a self,
        request: &ModelRequest,
        runtime: &super::request::RuntimeRequest,
        working_directory: &std::path::Path,
        cancellation: &CancellationToken,
    ) -> Option<&'a invocation::CacheAnchor> {
        if runtime.cyber_access_program().is_some()
            || !matches!(request.options().cache(), peritus_model_protocol::CachePolicy::Automatic)
        {
            return None;
        }
        if self.cache_anchor.get().is_none() {
            let anchor = invocation::create_cache_anchor(
                &self.config,
                self.transport.as_ref(),
                working_directory,
                cancellation,
            )
            .await;
            if anchor.is_err() {
                eprintln!(
                    "peritus: Codex cache routing setup unavailable; using isolated uncached turns"
                );
            }
            let _ = self.cache_anchor.set(anchor.ok());
        }
        self.cache_anchor
            .get()
            .and_then(|anchor| anchor.as_ref())
            .filter(|anchor| anchor.available.load(Ordering::Acquire))
    }

    fn failed_process(
        &self,
        request: &ModelRequest,
        decoded: &Result<super::output::RuntimeTurn, DecodeFailure>,
        usage: peritus_model_protocol::UsageCounters,
        had_output: bool,
        cancellation: CancellationToken,
    ) -> Result<OwnedModelStream, ProviderCoreError> {
        let stream = match decoded {
            Err(DecodeFailure::Incomplete) if had_output => CodexRuntimeStream::failed(
                request.model().clone(),
                failure(
                    self.config.profile().provider().clone(),
                    FailureCategory::Transport,
                    TransportPhase::StreamObserved,
                    OutcomeCertainty::AcceptedPartial,
                    Retryability::Never,
                    "openai.codex_runtime.interrupted",
                    None,
                )?,
                b"openai-codex-runtime-interrupted",
                true,
            )?,
            Err(reason) if !matches!(reason, DecodeFailure::Incomplete) => decode_failure(
                request.model().clone(),
                self.config.profile().provider().clone(),
                reason,
                usage,
            )?,
            Ok(_) | Err(_) => CodexRuntimeStream::failed_observed(
                request.model().clone(),
                failure(
                    self.config.profile().provider().clone(),
                    FailureCategory::AmbiguousAcceptance,
                    TransportPhase::SendingBody,
                    OutcomeCertainty::MaybeAccepted,
                    Retryability::CallerDecision,
                    "openai.codex_runtime.process",
                    None,
                )?,
                b"openai-codex-runtime-process",
                false,
                usage,
            )?,
        };
        Ok(OwnedModelStream::new(stream, cancellation))
    }
}

fn private_runtime_directory() -> std::io::Result<tempfile::TempDir> {
    if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
        && path.is_absolute()
        && std::fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        && let Ok(directory) = tempfile::Builder::new().prefix("peritus-openai-").tempdir_in(path)
    {
        return Ok(directory);
    }
    tempfile::Builder::new().prefix("peritus-openai-").tempdir()
}

impl ModelProvider for CodexRuntimeProvider {
    fn context_input_target(&self) -> u64 {
        self.profile()
            .limits()
            .max_input_tokens()
            .min((super::request::MAX_INPUT_CHARACTERS as u64) / 6)
    }

    fn context_input_fits(
        &self,
        messages: &[peritus_model_protocol::Message],
        tools: &[peritus_model_protocol::ToolDefinition],
    ) -> Result<bool, ProviderCoreError> {
        super::request::context_fits(messages, tools)
    }

    fn supports_reasoning_effort(&self, _effort: peritus_model_protocol::ReasoningEffort) -> bool {
        self.profile()
            .capabilities()
            .supports(peritus_model_protocol::Capability::ReasoningControls)
    }

    fn discover_models<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<
        'a,
        Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, ProviderCoreError>,
    > {
        Box::pin(peritus_provider_core::catalog::discover_account_models(
            self.config.executable().as_path(),
            peritus_provider_core::catalog::AccountCatalog::Codex,
            cancellation,
        ))
    }

    fn select_model(
        &self,
        model: peritus_model_protocol::ModelName,
    ) -> Result<std::sync::Arc<dyn ModelProvider>, ProviderCoreError> {
        let profile =
            peritus_provider_core::catalog::selected_profile(self.config.profile(), model)?;
        let config = CodexRuntimeConfig::new(
            self.config.executable().clone(),
            profile,
            self.config.process_limits(),
        )?;
        Ok(std::sync::Arc::new(Self::new(config)))
    }

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

impl fmt::Debug for CodexRuntimeProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexRuntimeProvider")
            .field("config", &self.config)
            .field("authenticated", &self.authenticated.load(Ordering::Acquire))
            .field("transport", &"[private owned process transport]")
            .field("working_directory", &"[private lazy directory]")
            .field("cache_anchor", &self.cache_anchor.get().is_some())
            .finish()
    }
}
