//! Named-service composition through the same native adapters used by ordinary daemon routes.

use peritus_model_protocol::{ModelName, ModelRequest, ProviderName, ProviderProfile, WireDialect};
use peritus_provider_core::{
    BoxFuture, CancellationToken, CredentialReference, CredentialSource, Endpoint, FramingLimits,
    HeaderName, HttpHeaders, HttpLimits, ModelProvider, OwnedModelStream, ProviderAvailability,
    ProviderCoreError, ReqwestTransport, RetryPolicy,
    catalog::{DiscoveredModel, selected_profile, unavailable},
    hosted::{HostedService, discover_hosted_models},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(super) struct HostedProvider {
    service: HostedService,
    credential: CredentialReference,
    credentials: Arc<dyn CredentialSource>,
    adapter: Arc<dyn ModelProvider>,
    catalog: Arc<Mutex<BTreeMap<String, DiscoveredModel>>>,
}

impl HostedProvider {
    pub(super) fn new(
        service: HostedService,
        credential: CredentialReference,
        profile: ProviderProfile,
        credentials: Arc<dyn CredentialSource>,
    ) -> Result<Self, ProviderCoreError> {
        let adapter = build(service, &credential, profile, &credentials)?;
        Ok(Self { service, credential, credentials, adapter, catalog: Arc::default() })
    }
}

impl ModelProvider for HostedProvider {
    fn profile(&self) -> &ProviderProfile {
        self.adapter.profile()
    }

    fn route(&self) -> peritus_provider_core::ProviderRoute {
        peritus_provider_core::ProviderRoute::CompatibleApi
    }

    fn availability(&self) -> ProviderAvailability {
        self.adapter.availability()
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.adapter.start(request, cancellation)
    }

    fn discover_models<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Vec<DiscoveredModel>, ProviderCoreError>> {
        Box::pin(async move {
            let transport = ReqwestTransport::production()?;
            let headers = || {
                HttpHeaders::new(
                    vec![self.credentials.resolve(&self.credential)?.into_header(
                        HeaderName::new("authorization".to_owned())?,
                        Some("Bearer "),
                    )?],
                    HttpLimits::PRODUCTION,
                )
            };
            let models = discover_hosted_models(
                self.service,
                &transport,
                &headers,
                HttpLimits::PRODUCTION,
                cancellation,
            )
            .await?;
            *self.catalog.lock().map_err(|_| unavailable("hosted model catalog lock failed"))? =
                models.iter().map(|model| (model.id.as_str().to_owned(), model.clone())).collect();
            Ok(models)
        })
    }

    fn select_model(&self, model: ModelName) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        let metadata = self
            .catalog
            .lock()
            .map_err(|_| unavailable("hosted model catalog lock failed"))?
            .get(model.as_str())
            .cloned();
        let dialect = if model == *self.profile().model() {
            self.profile().dialect()
        } else if self.service.mixed_protocols() {
            metadata.as_ref().and_then(|model| model.dialect).ok_or_else(|| unavailable("selected model has no protocol metadata; use provider setup to choose its documented API"))?
        } else {
            WireDialect::CompatibleChatCompletions
        };
        if metadata.as_ref().and_then(|model| model.tools) == Some(false) {
            return Err(ProviderCoreError::unsupported_capability(
                "selected model advertises no tool-calling support; choose a tool-capable model",
            ));
        }
        let selected = selected_profile(self.profile(), model)?;
        let name = match dialect {
            WireDialect::OpenAiResponses => "openai",
            WireDialect::AnthropicMessages => "anthropic",
            WireDialect::GeminiGenerateContentV1 => "google",
            _ => "compatible",
        };
        let mut capabilities: Vec<_> = selected
            .capabilities()
            .iter()
            .filter_map(|(capability, state)| {
                (state == peritus_model_protocol::CapabilityState::Supported
                    && capability != peritus_model_protocol::Capability::ReasoningReplay)
                    .then_some(capability)
            })
            .collect();
        if dialect != WireDialect::CompatibleResponses {
            capabilities.push(peritus_model_protocol::Capability::ReasoningReplay);
        }
        let capabilities = peritus_model_protocol::CapabilityMatrix::new(&capabilities, &[])
            .map_err(|_| unavailable("selected model capabilities are invalid"))?;
        let profile = ProviderProfile::new(
            selected.profile_id(),
            selected.revision(),
            ProviderName::new(name.to_owned())
                .map_err(|_| unavailable("hosted provider name is invalid"))?,
            selected.model().clone(),
            dialect,
            capabilities,
            selected.provenance(),
            selected.limits(),
            selected.output_limit_enforcement(),
            selected.state_mode(),
            selected.resume_kind(),
            selected.cancellation_kind(),
        )
        .map_err(|_| unavailable("selected model protocol contradicts the configured profile"))?;
        let mut provider = Self::new(
            self.service,
            self.credential.clone(),
            profile,
            Arc::clone(&self.credentials),
        )?;
        provider.catalog = Arc::clone(&self.catalog);
        Ok(Arc::new(provider))
    }
}

fn build(
    service: HostedService,
    credential: &CredentialReference,
    profile: ProviderProfile,
    credentials: &Arc<dyn CredentialSource>,
) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
    use peritus_provider_anthropic::{AnthropicClient, AnthropicConfig};
    use peritus_provider_compatible::{
        CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile,
    };
    use peritus_provider_google::{GoogleClient, GoogleConfig};
    let endpoint = Endpoint::new(service.route(profile.dialect())?.endpoint.to_owned())?;
    let retry = RetryPolicy::new(
        2,
        [
            Duration::from_millis(100),
            Duration::from_secs(2),
            Duration::from_secs(2),
            Duration::from_secs(10),
        ],
        64 * 1024 * 1024,
    )?;
    match profile.dialect() {
        WireDialect::OpenAiResponses => {
            let config = peritus_provider_openai::OpenAiConfig::opencode_gateway(
                endpoint,
                credential.clone(),
            )?;
            Ok(Arc::new(peritus_provider_openai::OpenAiProvider::new(
                config,
                profile,
                Arc::clone(credentials),
            )?))
        }
        WireDialect::AnthropicMessages => {
            let config = AnthropicConfig::new(
                endpoint,
                credential.clone(),
                profile,
                Vec::new(),
                HttpLimits::PRODUCTION,
                FramingLimits::PRODUCTION,
                retry,
            )?
            .with_opencode_gateway()?;
            Ok(Arc::new(AnthropicClient::new(
                config,
                Box::new(super::providers::SharedCredentialSource(Arc::clone(credentials))),
            )?))
        }
        WireDialect::GeminiGenerateContentV1 => {
            let config = GoogleConfig::opencode_gateway(
                endpoint,
                credential.clone(),
                profile,
                HttpLimits::PRODUCTION,
                FramingLimits::PRODUCTION,
                retry,
            )?;
            Ok(Arc::new(GoogleClient::new(
                config,
                Box::new(super::providers::SharedCredentialSource(Arc::clone(credentials))),
            )?))
        }
        WireDialect::CompatibleResponses | WireDialect::CompatibleChatCompletions => {
            let config =
                CompatibleConfig::new(endpoint, CompatibleAuth::bearer(credential.clone())?)?
                    .with_hosted_service(service)?;
            let profile = if profile.dialect() == WireDialect::CompatibleResponses {
                CompatibleProfile::responses(profile)?
            } else {
                CompatibleProfile::hosted_chat_completions(profile)?
            };
            Ok(Arc::new(CompatibleClient::new(config, profile, Arc::clone(credentials))?))
        }
        _ => Err(ProviderCoreError::unsupported_capability("hosted protocol has no adapter")),
    }
}
