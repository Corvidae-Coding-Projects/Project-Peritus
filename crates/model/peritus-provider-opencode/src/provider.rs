//! Owned vendor inference and one bounded format correction, with no native agent authority.

use peritus_model_protocol::{
    FailureCategory, ModelEvent, ModelFailure, ModelName, ModelRequest, OutcomeCertainty,
    OutputLimitEnforcement, ProviderProfile, RedactedDiagnostic, ResumeKind, Retryability,
    StateMode, TransportPhase, UsageCounters, WireDialect,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, CredentialReference, CredentialSource, ModelProvider,
    OwnedModelStream, ProcessLimits, ProcessOutput, ProcessTransport, ProviderAvailability,
    ProviderCoreError, ProviderCoreErrorKind, TokioProcessTransport, catalog::DiscoveredModel,
    hosted::HostedService, validate_request_profile,
};
use std::sync::{Arc, Mutex};

/// Official `OpenCode` as an inert Zen/Go router using the Peritus credential broker.
pub struct OpenCodeRuntimeProvider {
    executable_path: Option<std::path::PathBuf>,
    service: HostedService,
    credential: CredentialReference,
    credentials: Arc<dyn CredentialSource>,
    profile: ProviderProfile,
    discover_limits: bool,
    catalog: Mutex<Vec<DiscoveredModel>>,
    catalog_loading: tokio::sync::Mutex<()>,
    request_layout: super::OpenCodeRequestLayout,
}

struct TurnState {
    corrections: Vec<ModelEvent>,
    rejected_usage: Option<UsageCounters>,
    prompt: Vec<u8>,
}

enum OutputDisposition {
    Retry,
    Complete(OwnedModelStream),
}

impl OpenCodeRuntimeProvider {
    /// Constructs an explicitly selected, pinned executable route.
    ///
    /// # Errors
    /// Rejects non-OpenCode services and profiles claiming provider state or exact token control.
    pub fn new(
        executable: &super::OpenCodeExecutable,
        service: HostedService,
        credential: CredentialReference,
        credentials: Arc<dyn CredentialSource>,
        profile: ProviderProfile,
    ) -> Result<Self, ProviderCoreError> {
        Self::with_executable_path(
            Some(executable.as_path().to_owned()),
            service,
            credential,
            credentials,
            profile,
        )
    }

    /// Selects an explicit path or defers native discovery until this route is used.
    ///
    /// A missing secondary executable must not prevent unrelated providers from starting.
    /// Paths are pinned and the supported version is checked before native submission.
    ///
    /// # Errors
    /// Rejects an invalid service or profile without opening a native process.
    pub fn with_executable_path(
        executable_path: Option<std::path::PathBuf>,
        service: HostedService,
        credential: CredentialReference,
        credentials: Arc<dyn CredentialSource>,
        profile: ProviderProfile,
    ) -> Result<Self, ProviderCoreError> {
        super::service_id(service)?;
        if profile.provider().as_str() != "opencode"
            || profile.dialect() != WireDialect::OpenCodeRuntime
            || profile.output_limit_enforcement() != OutputLimitEnforcement::Advisory
            || profile.state_mode() != StateMode::StatelessReplay
            || profile.resume_kind() != ResumeKind::Unsupported
        {
            return Err(ProviderCoreError::configuration(
                "opencode_runtime_profile",
                "OpenCode requires its stateless, advisory-output runtime profile",
            ));
        }
        if profile.capabilities().iter().any(|(capability, state)| {
            state != peritus_model_protocol::CapabilityState::Unsupported
                && !matches!(
                    capability,
                    peritus_model_protocol::Capability::Streaming
                        | peritus_model_protocol::Capability::ToolCalls
                        | peritus_model_protocol::Capability::ParallelToolCalls
                        | peritus_model_protocol::Capability::UsageDetail
                        | peritus_model_protocol::Capability::PromptCaching
                )
        }) {
            return Err(ProviderCoreError::configuration(
                "opencode_runtime_profile",
                "OpenCode runtime profile advertises an unsupported control or input type",
            ));
        }
        Ok(Self {
            executable_path,
            service,
            credential,
            credentials,
            profile,
            discover_limits: true,
            catalog: Mutex::new(Vec::new()),
            catalog_loading: tokio::sync::Mutex::new(()),
            request_layout: super::OpenCodeRequestLayout::default(),
        })
    }

    /// Chooses catalog token capacities or explicitly configured fixed token limits.
    #[must_use]
    pub const fn with_discovered_limits(mut self, discover: bool) -> Self {
        self.discover_limits = discover;
        self
    }

    /// Selects the adapter layout for new turns. This never retries an admitted request.
    /// The once-only layout is the default; legacy remains an explicit independent rollback.
    #[must_use]
    pub const fn with_request_layout(mut self, layout: super::OpenCodeRequestLayout) -> Self {
        self.request_layout = layout;
        self
    }

    fn model_metadata(&self) -> Result<Option<DiscoveredModel>, ProviderCoreError> {
        if !self.discover_limits {
            return Ok(None);
        }
        let service = super::service_id(self.service)?;
        let id = model_id(self.profile.model(), service)?;
        Ok(self
            .catalog
            .lock()
            .map_err(|_| catalog_error())?
            .iter()
            .find(|model| model.id.as_str() == id)
            .cloned())
    }

    async fn executable(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<super::OpenCodeExecutable, ProviderCoreError> {
        match self.executable_path.as_ref() {
            Some(path) => super::OpenCodeExecutable::pin(path),
            None => super::OpenCodeExecutable::discover_supported(cancellation).await,
        }
    }

    async fn turn(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> Result<OwnedModelStream, ProviderCoreError> {
        validate_request_profile(&self.profile, &request)?;
        let service = super::service_id(self.service)?;
        let model = model_id(request.model(), service)?;
        let empty = self.catalog.lock().map_err(|_| catalog_error())?.is_empty();
        if empty {
            self.discover_models(&cancellation).await?;
        }
        if !self
            .catalog
            .lock()
            .map_err(|_| catalog_error())?
            .iter()
            .any(|entry| entry.id.as_str() == model)
        {
            return Err(ProviderCoreError::invalid_request(
                "opencode_runtime_model",
                "selected model is absent from the official OpenCode catalog; refresh models or choose an exact advertised ID",
            ));
        }
        let runtime = super::request::encode(&request, self.request_layout)?;
        let mut projections = Vec::new();
        let stream = self.turn_encoded(request, cancellation, runtime, &mut projections).await?;
        Ok(projections.into_iter().fold(stream, OwnedModelStream::with_projection))
    }

    async fn turn_encoded(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
        runtime: super::request::RuntimeRequest,
        projections: &mut Vec<peritus_provider_core::RequestProjection>,
    ) -> Result<OwnedModelStream, ProviderCoreError> {
        let service = super::service_id(self.service)?;
        let model = model_id(request.model(), service)?;
        let credential = self.credentials.resolve(&self.credential)?;
        let system = std::str::from_utf8(&runtime.system).map_err(|_| super::malformed())?;
        let private = super::isolation::Isolation::new(service, &credential, system)?;
        let executable = self.executable(&cancellation).await?;
        executable.require_supported(&private, &cancellation).await?;
        let args = vec![
            "--print-logs".into(),
            "--log-level".into(),
            "ERROR".into(),
            "run".into(),
            "--pure".into(),
            "--format".into(),
            "json".into(),
            "--agent".into(),
            "peritus-router".into(),
            "--title".into(),
            "Peritus inference".into(),
            "--model".into(),
            format!("{service}/{model}"),
        ];
        let mut state = TurnState {
            corrections: Vec::new(),
            rejected_usage: None,
            prompt: runtime.prompt.clone(),
        };
        loop {
            if peritus_provider_core::efficiency_diagnostics_enabled() {
                let projection = peritus_provider_core::RequestProjection::measure(
                    self.request_layout.projection_layout(),
                    &[&runtime.system, &state.prompt],
                )
                .with_reason(if state.rejected_usage.is_some() {
                    "format_correction"
                } else {
                    "primary"
                });
                projections.push(runtime.components.as_ref().map_or_else(
                    || projection.clone(),
                    |parts| projection.clone().with_components(parts),
                ));
            }
            let process = private
                .process(
                    &executable,
                    args.clone(),
                    state.prompt.clone(),
                    ProcessLimits::PRODUCTION,
                )?
                .with_stderr_observer(super::output::observe_stderr);
            let output = match TokioProcessTransport.run(process, &cancellation).await {
                Ok(output) => output,
                Err(error) if error.kind() == ProviderCoreErrorKind::Cancelled => {
                    return Ok(OwnedModelStream::new(
                        super::stream::OpenCodeRuntimeStream::cancelled(request.model().clone())?,
                        cancellation,
                    ));
                }
                Err(error) if error.kind() == ProviderCoreErrorKind::Connect => return Err(error),
                Err(error) if super::output::observed_failure(&error).is_some() => {
                    let failure =
                        super::output::observed_failure(&error).ok_or_else(super::malformed)?;
                    return self.failed(
                        &request,
                        cancellation,
                        &failure,
                        OutcomeCertainty::Terminal,
                        state.corrections,
                    );
                }
                Err(error) => {
                    let (category, code) = if error.operation() == "process_timeout" {
                        (FailureCategory::Timeout, "opencode.runtime.timeout")
                    } else {
                        (FailureCategory::AmbiguousAcceptance, "opencode.runtime.transport")
                    };
                    return self.failed(
                        &request,
                        cancellation,
                        &super::output::NativeFailure { category, status: None, code },
                        OutcomeCertainty::MaybeAccepted,
                        state.corrections,
                    );
                }
            };
            match self.accept_output(
                &request,
                &runtime,
                &output,
                &cancellation,
                &mut state,
                projections.last_mut(),
            )? {
                OutputDisposition::Retry => {}
                OutputDisposition::Complete(stream) => return Ok(stream),
            }
        }
    }

    fn accept_output(
        &self,
        request: &ModelRequest,
        runtime: &super::request::RuntimeRequest,
        output: &ProcessOutput,
        cancellation: &CancellationToken,
        state: &mut TurnState,
        mut projection: Option<&mut peritus_provider_core::RequestProjection>,
    ) -> Result<OutputDisposition, ProviderCoreError> {
        // Native framing and failures stay authoritative. Only a completed, successful,
        // tool-disabled inference with a rejected model envelope can be regenerated.
        let decoded = super::output::decode(output.stdout(), request, runtime);
        if let Some(projection) = projection.as_mut() {
            match &decoded {
                Ok(super::output::Decoded::Turn(turn)) => projection.observe_native_result(
                    if output.exit().success() { "completed" } else { "incomplete_exit" },
                    Some(turn.usage),
                ),
                Ok(super::output::Decoded::Rejected { usage, .. }) => {
                    projection.observe_native_result("rejected_envelope", Some(*usage));
                }
                Ok(super::output::Decoded::Failure(_)) => {
                    projection.observe_native_result("failed", None);
                }
                Err(_) => projection.observe_native_result("malformed", None),
            }
        }
        match decoded {
            Ok(super::output::Decoded::Rejected { reason, usage, rejected })
                if output.exit().success() =>
            {
                if let Some(projection) = projection.as_mut() {
                    projection.observe_native_result("rejected_envelope", Some(usage));
                }
                state.corrections.push(
                    reason.audit(if state.rejected_usage.is_some() { 2 } else { 1 }, &rejected)?,
                );
                if state.rejected_usage.is_none() {
                    state.rejected_usage = Some(usage);
                    state.prompt = reason.correction(&runtime.prompt);
                    return Ok(OutputDisposition::Retry);
                }
                self.failed(
                    request,
                    cancellation.clone(),
                    &super::output::NativeFailure {
                        category: FailureCategory::MalformedPayload,
                        status: None,
                        code: reason.code(),
                    },
                    OutcomeCertainty::Terminal,
                    std::mem::take(&mut state.corrections),
                )
                .map(OutputDisposition::Complete)
            }
            Ok(super::output::Decoded::Failure(error)) => {
                if let Some(projection) = projection.as_mut() {
                    projection.observe_native_result("failed", None);
                }
                self.failed(
                    request,
                    cancellation.clone(),
                    &error,
                    OutcomeCertainty::Terminal,
                    std::mem::take(&mut state.corrections),
                )
                .map(OutputDisposition::Complete)
            }
            Ok(super::output::Decoded::Turn(mut turn)) if output.exit().success() => {
                if let Some(projection) = projection.as_mut() {
                    projection.observe_native_result("completed", Some(turn.usage));
                }
                if let Some(previous) = state.rejected_usage {
                    turn.usage = super::recovery::combined_usage(previous, turn.usage)?;
                }
                state.corrections.append(&mut turn.repairs);
                turn.repairs = std::mem::take(&mut state.corrections);
                let stream = super::stream::OpenCodeRuntimeStream::completed(
                    request,
                    turn,
                    output.stdout(),
                )?;
                Ok(OutputDisposition::Complete(OwnedModelStream::new(stream, cancellation.clone())))
            }
            Ok(super::output::Decoded::Turn(_)) => self
                .failed(
                    request,
                    cancellation.clone(),
                    &super::output::NativeFailure {
                        category: FailureCategory::IncompleteStream,
                        status: None,
                        code: "opencode.runtime.incomplete_exit",
                    },
                    OutcomeCertainty::MaybeAccepted,
                    std::mem::take(&mut state.corrections),
                )
                .map(OutputDisposition::Complete),
            Ok(super::output::Decoded::Rejected { .. }) | Err(_) => self
                .failed(
                    request,
                    cancellation.clone(),
                    &super::output::NativeFailure {
                        category: FailureCategory::MalformedPayload,
                        status: None,
                        code: "opencode.runtime.malformed_output",
                    },
                    OutcomeCertainty::MaybeAccepted,
                    std::mem::take(&mut state.corrections),
                )
                .map(OutputDisposition::Complete),
        }
    }

    fn failed(
        &self,
        request: &ModelRequest,
        cancellation: CancellationToken,
        native: &super::output::NativeFailure,
        certainty: OutcomeCertainty,
        corrections: Vec<ModelEvent>,
    ) -> Result<OwnedModelStream, ProviderCoreError> {
        let &super::output::NativeFailure { category, status, code } = native;
        let retryability = if certainty == OutcomeCertainty::MaybeAccepted {
            Retryability::CallerDecision
        } else if matches!(
            category,
            FailureCategory::RateLimited
                | FailureCategory::TransientProvider
                | FailureCategory::MalformedPayload
        ) {
            Retryability::SafeNewRequest
        } else {
            Retryability::Never
        };
        let diagnostic = RedactedDiagnostic::new(code.to_owned(), None, None, None)
            .map_err(|_| super::malformed())?;
        let failure = ModelFailure::new(
            self.profile.provider().clone(),
            category,
            if certainty == OutcomeCertainty::Terminal {
                TransportPhase::Completed
            } else {
                TransportPhase::SendingBody
            },
            certainty,
            retryability,
            status,
            None,
            None,
            diagnostic,
        );
        Ok(OwnedModelStream::new(
            super::stream::OpenCodeRuntimeStream::failed(
                request.model().clone(),
                failure,
                b"opencode-runtime-failure",
                corrections,
            )?,
            cancellation,
        ))
    }
}

impl ModelProvider for OpenCodeRuntimeProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn request_profile(&self, output_tokens: u64) -> Result<ProviderProfile, ProviderCoreError> {
        let Some(model) = self.model_metadata()? else {
            return Ok(self.profile.clone());
        };
        peritus_provider_core::catalog::profile_with_model_limits(
            &self.profile,
            &model,
            output_tokens,
        )
    }

    fn output_tokens_for_input(&self, input: u64, output: u64) -> Result<u64, ProviderCoreError> {
        let Some(model) = self.model_metadata()? else {
            return Ok(output.min(self.profile.limits().max_output_tokens()));
        };
        peritus_provider_core::catalog::output_tokens_for_input(
            &self.profile,
            &model,
            input,
            output,
        )
    }

    fn prepare_context<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async {
            let _loading = self.catalog_loading.lock().await;
            let empty = self.catalog.lock().map_err(|_| catalog_error())?.is_empty();
            if empty {
                self.discover_models(cancellation).await?;
            }
            Ok(())
        })
    }

    fn availability(&self) -> ProviderAvailability {
        let installed = self
            .executable_path
            .as_ref()
            .map_or_else(super::OpenCodeExecutable::discover, super::OpenCodeExecutable::pin)
            .is_ok();
        if installed && self.credentials.resolve(&self.credential).is_ok() {
            ProviderAvailability::CredentialPresent
        } else {
            ProviderAvailability::Unavailable
        }
    }

    fn discover_models<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Vec<DiscoveredModel>, ProviderCoreError>> {
        Box::pin(async move {
            let credential = self.credentials.resolve(&self.credential)?;
            let executable = self.executable(cancellation).await?;
            let models =
                super::discover_models(&executable, self.service, &credential, cancellation)
                    .await?;
            let mut catalog = self.catalog.lock().map_err(|_| catalog_error())?;
            models.clone_into(&mut catalog);
            Ok(models)
        })
    }

    fn select_model(&self, model: ModelName) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        let service = super::service_id(self.service)?;
        let id = model_id(&model, service)?;
        let metadata = self
            .catalog
            .lock()
            .map_err(|_| catalog_error())?
            .iter()
            .find(|entry| entry.id.as_str() == id)
            .cloned();
        if metadata.as_ref().and_then(|entry| entry.tools) == Some(false) {
            return Err(ProviderCoreError::unsupported_capability(
                "selected OpenCode model advertises no tool-calling support",
            ));
        }
        let profile = peritus_provider_core::catalog::selected_profile(&self.profile, model)?;
        let selected = Self::with_executable_path(
            self.executable_path.clone(),
            self.service,
            self.credential.clone(),
            Arc::clone(&self.credentials),
            profile,
        )?
        .with_discovered_limits(self.discover_limits)
        .with_request_layout(self.request_layout);
        let models = self.catalog.lock().map_err(|_| catalog_error())?;
        let mut catalog = selected.catalog.lock().map_err(|_| catalog_error())?;
        catalog.extend(models.iter().cloned());
        drop(catalog);
        drop(models);
        Ok(Arc::new(selected))
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(self.turn(request, cancellation))
    }
}

fn model_id<'a>(model: &'a ModelName, service: &str) -> Result<&'a str, ProviderCoreError> {
    let id = model.as_str();
    if let Some((prefix, id)) = id.split_once('/') {
        if prefix != service || id.is_empty() {
            return Err(ProviderCoreError::invalid_request(
                "opencode_runtime_model",
                "model prefix does not match the selected Zen/Go service",
            ));
        }
        Ok(id)
    } else {
        Ok(id)
    }
}

const fn catalog_error() -> ProviderCoreError {
    peritus_provider_core::catalog::unavailable("OpenCode catalog state is unavailable")
}
