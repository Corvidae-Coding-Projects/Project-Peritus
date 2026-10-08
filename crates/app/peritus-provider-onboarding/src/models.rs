//! Metadata-only provider discovery for synchronous setup, isolated from an ambient async runtime.

use crate::{AccountProvider, OnboardingError};
use peritus_product_state::ProviderKind;
use peritus_provider_core::{
    CancellationToken, Credential, Endpoint, Header, HeaderName, HttpHeaders, HttpLimits,
    ReqwestTransport,
    catalog::{
        AccountCatalog, CatalogDialect, DiscoveredModel, derive_compatible_catalog_endpoint,
        discover_account_models, discover_http_models, unavailable,
    },
};

impl AccountProvider {
    /// Queries the official executable's advertised models without an inference prompt.
    ///
    /// # Errors
    /// Returns an unsupported runtime or bounded discovery failure, not a substitute list.
    pub fn discover_models(&self) -> Result<Vec<String>, OnboardingError> {
        let kind = if self.kind() == ProviderKind::CodexAccount {
            AccountCatalog::Codex
        } else {
            AccountCatalog::Claude
        };
        run(async {
            discover_account_models(self.executable(), kind, &CancellationToken::new()).await
        })
        .map(|models| models.into_iter().map(|model| model.id.as_str().to_owned()).collect())
    }
}

pub fn direct(
    kind: ProviderKind,
    endpoint: Option<&str>,
    catalog_endpoint: Option<&str>,
    header: Option<&str>,
    credential: &peritus_secrets::SecretMaterial,
) -> Result<Vec<DiscoveredModel>, OnboardingError> {
    let (endpoint, dialect, name, prefix) = match kind {
        ProviderKind::OpenAiApi => (
            "https://api.openai.com/v1/models".to_owned(),
            CatalogDialect::OpenAi,
            "authorization",
            Some("Bearer "),
        ),
        ProviderKind::AnthropicApi => (
            format!(
                "{}/v1/models?limit=1000",
                endpoint.ok_or(OnboardingError::ModelCatalog)?.trim_end_matches('/')
            ),
            CatalogDialect::Anthropic,
            "x-api-key",
            None,
        ),
        ProviderKind::GoogleGeminiApi => (
            format!(
                "{}/v1/models?pageSize=1000",
                endpoint.ok_or(OnboardingError::ModelCatalog)?.trim_end_matches('/')
            ),
            CatalogDialect::GoogleV1,
            "x-goog-api-key",
            None,
        ),
        ProviderKind::CompatibleEndpoint => {
            let endpoint = compatible_catalog_endpoint(
                endpoint.ok_or(OnboardingError::ModelCatalog)?,
                catalog_endpoint,
            )?
            .ok_or(OnboardingError::ModelCatalog)?;
            (
                endpoint.as_str().to_owned(),
                CatalogDialect::OpenAi,
                header.unwrap_or("authorization"),
                if header.is_none() { Some("Bearer ") } else { None },
            )
        }
        _ => {
            let service = kind
                .hosted_service()
                .and_then(peritus_provider_core::hosted::HostedService::parse)
                .ok_or(OnboardingError::UnsupportedProvider)?;
            (
                service.models_endpoint().to_owned(),
                CatalogDialect::OpenAi,
                "authorization",
                Some("Bearer "),
            )
        }
    };
    run(async {
        let endpoint = Endpoint::new(endpoint)?;
        let transport = ReqwestTransport::production()?;
        let headers = || {
            let credential = credential.expose(|bytes| Credential::new(bytes.to_vec()))?;
            let mut headers =
                vec![credential.into_header(HeaderName::new(name.to_owned())?, prefix)?];
            if dialect == CatalogDialect::Anthropic {
                headers.push(Header::new(
                    HeaderName::new("anthropic-version".to_owned())?,
                    b"2023-06-01".to_vec(),
                )?);
            }
            HttpHeaders::new(headers, HttpLimits::PRODUCTION)
        };
        if let Some(service) =
            kind.hosted_service().and_then(peritus_provider_core::hosted::HostedService::parse)
        {
            return peritus_provider_core::hosted::discover_hosted_models(
                service,
                &transport,
                &headers,
                HttpLimits::PRODUCTION,
                &CancellationToken::new(),
            )
            .await;
        }
        discover_http_models(
            &transport,
            &endpoint,
            dialect,
            &headers,
            HttpLimits::PRODUCTION,
            &CancellationToken::new(),
        )
        .await
    })
}

pub(crate) fn compatible_catalog_endpoint(
    inference_endpoint: &str,
    configured_catalog_endpoint: Option<&str>,
) -> Result<Option<Endpoint>, OnboardingError> {
    let inference = Endpoint::new(inference_endpoint.to_owned())
        .map_err(OnboardingError::ModelDiscovery)?;
    let catalog = match configured_catalog_endpoint {
        Some(endpoint) => Some(
            Endpoint::new(endpoint.to_owned()).map_err(OnboardingError::ModelDiscovery)?,
        ),
        None => derive_compatible_catalog_endpoint(&inference)
            .map_err(OnboardingError::ModelDiscovery)?,
    };
    if catalog.as_ref().is_some_and(|endpoint| !inference.same_origin(endpoint)) {
        return Err(OnboardingError::ModelDiscovery(unavailable(
            "direct compatible catalog endpoint must share the inference origin",
        )));
    }
    Ok(catalog)
}

fn run(
    future: impl Future<Output = Result<Vec<DiscoveredModel>, peritus_provider_core::ProviderCoreError>>
    + Send,
) -> Result<Vec<DiscoveredModel>, OnboardingError> {
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| OnboardingError::ModelCatalog)?;
                runtime.block_on(future).map_err(OnboardingError::ModelDiscovery)
            })
            .join()
            .map_err(|_| OnboardingError::ModelCatalog)?
    })
}
