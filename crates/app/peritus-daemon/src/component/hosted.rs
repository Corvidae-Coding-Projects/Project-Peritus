//! Named-service composition through the same native adapters used by ordinary daemon routes.

use peritus_model_protocol::{ModelName, ModelRequest, ProviderProfile, WireDialect};
use peritus_provider_core::{
    BoxFuture, CancellationToken, CredentialReference, CredentialSource, Endpoint, FramingLimits,
    HeaderName, HttpHeaders, HttpLimits, ModelProvider, OwnedModelStream, ProviderAvailability,
    ProviderCoreError, ReqwestTransport, RetryPolicy,
    catalog::{DiscoveredModel, unavailable},
    hosted::{HostedService, discover_hosted_models, enrich_hosted_models},
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
    catalog: Arc<HostedCatalog>,
}

#[derive(Default)]
struct HostedCatalog {
    state: Mutex<HostedCatalogState>,
    enrichment: Mutex<Option<HostedEnrichment>>,
}

#[derive(Default)]
struct HostedCatalogState {
    generation: u64,
    models: BTreeMap<String, DiscoveredModel>,
    progress: EnrichmentProgress,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum EnrichmentProgress {
    #[default]
    NotRequired,
    Pending,
    Complete,
    Unavailable,
}

struct HostedEnrichment {
    generation: u64,
    cancellation: CancellationToken,
    _task: tokio::task::JoinHandle<()>,
}

impl HostedCatalog {
    fn publish_inventory(
        &self,
        mixed_protocols: bool,
        models: &[DiscoveredModel],
    ) -> Result<u64, ProviderCoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("hosted model catalog lock failed"))?;
        let generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| unavailable("hosted model catalog generation overflowed"))?;
        let previous = core::mem::take(&mut state.models);
        state.models = models
            .iter()
            .map(|model| {
                let mut retained = model.clone();
                if let Some(prior) = previous.get(model.id.as_str()) {
                    retained.dialect = retained.dialect.or(prior.dialect);
                    retained.tools = retained.tools.or(prior.tools);
                    retained.input_tokens = retained.input_tokens.or(prior.input_tokens);
                    retained.output_tokens = retained.output_tokens.or(prior.output_tokens);
                }
                (model.id.as_str().to_owned(), retained)
            })
            .collect();
        state.generation = generation;
        state.progress =
            if mixed_protocols { EnrichmentProgress::Pending } else { EnrichmentProgress::NotRequired };
        drop(state);

        if let Some(previous) = self
            .enrichment
            .lock()
            .map_err(|_| unavailable("hosted model enrichment lock failed"))?
            .take()
        {
            let _ = previous.cancellation.cancel();
        }
        Ok(generation)
    }

    fn begin_enrichment(
        self: &Arc<Self>,
        service: HostedService,
        generation: u64,
        mut models: Vec<DiscoveredModel>,
    ) -> Result<(), ProviderCoreError> {
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let catalog = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            let result = async {
                let transport = ReqwestTransport::production()?;
                enrich_hosted_models(
                    service,
                    &transport,
                    HttpLimits::PRODUCTION,
                    &worker_cancellation,
                    &mut models,
                )
                .await?;
                Ok::<_, ProviderCoreError>(models)
            }
            .await;
            if let Some(catalog) = catalog.upgrade() {
                let _ = catalog.finish_enrichment(generation, result.ok());
            }
        });

        let pending = {
            let state = self
                .state
                .lock()
                .map_err(|_| unavailable("hosted model catalog lock failed"))?;
            state.generation == generation && state.progress == EnrichmentProgress::Pending
        };
        if !pending {
            let _ = cancellation.cancel();
            return Ok(());
        }
        *self
            .enrichment
            .lock()
            .map_err(|_| unavailable("hosted model enrichment lock failed"))? =
            Some(HostedEnrichment { generation, cancellation, _task: task });
        Ok(())
    }

    fn finish_enrichment(
        &self,
        generation: u64,
        models: Option<Vec<DiscoveredModel>>,
    ) -> Result<(), ProviderCoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("hosted model catalog lock failed"))?;
        if state.generation == generation {
            if let Some(models) = models {
                for model in models {
                    if let Some(current) = state.models.get_mut(model.id.as_str()) {
                        current.dialect = model.dialect;
                        current.tools = model.tools;
                        current.input_tokens = model.input_tokens;
                        current.output_tokens = model.output_tokens;
                    }
                }
                state.progress = EnrichmentProgress::Complete;
            } else {
                state.progress = EnrichmentProgress::Unavailable;
            }
        }
        drop(state);

        let mut active = self
            .enrichment
            .lock()
            .map_err(|_| unavailable("hosted model enrichment lock failed"))?;
        if active.as_ref().is_some_and(|task| task.generation == generation) {
            *active = None;
        }
        Ok(())
    }

    fn model(&self, id: &str) -> Result<Option<DiscoveredModel>, ProviderCoreError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| unavailable("hosted model catalog lock failed"))?
            .models
            .get(id)
            .cloned())
    }
}

impl Drop for HostedCatalog {
    fn drop(&mut self) {
        if let Ok(active) = self.enrichment.get_mut()
            && let Some(active) = active.take()
        {
            let _ = active.cancellation.cancel();
        }
    }
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

    fn validate_request(&self, request: &ModelRequest) -> Result<(), ProviderCoreError> {
        self.adapter.validate_request(request)
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
            let generation = self
                .catalog
                .publish_inventory(self.service.mixed_protocols(), &models)?;
            if self.service.mixed_protocols() {
                self.catalog.begin_enrichment(self.service, generation, models.clone())?;
            }
            Ok(models)
        })
    }

    fn select_model(&self, model: ModelName) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        if model != *self.profile().model() {
            return Err(unavailable(
                "selected model capacity or features are unresolved; choose it through provider setup",
            ));
        }
        let mut provider = Self::new(
            self.service,
            self.credential.clone(),
            self.profile().clone(),
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
    let retry = RetryPolicy::without_deadline(
        2,
        [Duration::from_millis(100), Duration::from_secs(2), Duration::from_secs(2)],
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
