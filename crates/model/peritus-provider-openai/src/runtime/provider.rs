//! One-turn ownership of the pre-authenticated Codex executable.

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
use super::output::{DecodeFailure, decode};
use super::stream::CodexRuntimeStream;
use failure::{decode_failure, failure};

/// Account-backed `OpenAI` provider constrained to the credential-owning `Codex` executable.
pub struct CodexRuntimeProvider {
    config: CodexRuntimeConfig,
    transport: Box<dyn ProcessTransport>,
    authenticated: AtomicBool,
}

/// Conventional client name for the account-backed Codex runtime provider.
pub type CodexRuntimeClient = CodexRuntimeProvider;

impl CodexRuntimeProvider {
    /// Creates a production provider backed by the pinned official executable.
    #[must_use]
    pub fn new(config: CodexRuntimeConfig) -> Self {
        Self {
            config,
            transport: Box::new(TokioProcessTransport),
            authenticated: AtomicBool::new(false),
        }
    }

    /// Returns the exact immutable runtime profile.
    #[must_use]
    pub const fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        self.config.profile()
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
            if let Err(error) = self.require_authenticated(&cancellation).await {
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
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
            let output = match invocation::run_turn(
                &self.config,
                self.transport.as_ref(),
                &request,
                &runtime,
                &cancellation,
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
            let decoded = decode(output.stdout(), &runtime.allowed_tools, runtime.max_calls);
            if !output.exit().success() {
                return self.failed_process(
                    &request,
                    &decoded,
                    !output.stdout().is_empty(),
                    cancellation,
                );
            }
            let turn = match decoded {
                Ok(turn) => turn,
                Err(reason) => {
                    let stream = decode_failure(
                        request.model().clone(),
                        self.config.profile().provider().clone(),
                        &reason,
                    )?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
            };
            let stream = CodexRuntimeStream::completed(&request, turn, output.stdout())?;
            Ok(OwnedModelStream::new(stream, cancellation))
        })
    }

    fn failed_process(
        &self,
        request: &ModelRequest,
        decoded: &Result<super::output::RuntimeTurn, DecodeFailure>,
        had_output: bool,
        cancellation: CancellationToken,
    ) -> Result<OwnedModelStream, ProviderCoreError> {
        let stream = match decoded {
            Err(
                reason @ (DecodeFailure::Authentication
                | DecodeFailure::Safety
                | DecodeFailure::RateLimited
                | DecodeFailure::Capacity
                | DecodeFailure::QuotaExhausted
                | DecodeFailure::ContextLimit
                | DecodeFailure::Reported),
            ) => decode_failure(
                request.model().clone(),
                self.config.profile().provider().clone(),
                reason,
            )?,
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
            _ => CodexRuntimeStream::failed(
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
            )?,
        };
        Ok(OwnedModelStream::new(stream, cancellation))
    }
}

impl ModelProvider for CodexRuntimeProvider {
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
            .finish()
    }
}
