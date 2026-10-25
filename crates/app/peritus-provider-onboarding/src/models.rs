//! Metadata-only provider discovery for synchronous setup, isolated from an ambient async runtime.

use crate::{AccountProvider, OnboardingError};
use peritus_product_state::ProviderKind;
use peritus_provider_core::{
    CancellationToken, Credential, Endpoint, Header, HeaderName, HttpHeaders, HttpLimits,
    ReqwestTransport,
    catalog::{
        AccountCatalog, CatalogDialect, DiscoveredModel, discover_account_models,
        discover_http_models,
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
    header: Option<&str>,
    credential: &peritus_secrets::SecretMaterial,
) -> Result<Vec<DiscoveredModel>, OnboardingError> {
    if matches!(kind, ProviderKind::OpenCodeZen | ProviderKind::OpenCodeGo) {
        return run(async {
            let service = if kind == ProviderKind::OpenCodeZen {
                peritus_provider_core::hosted::HostedService::OpenCodeZen
            } else {
                peritus_provider_core::hosted::HostedService::OpenCodeGo
            };
            let executable = peritus_provider_opencode::OpenCodeExecutable::discover_supported(
                &CancellationToken::new(),
            )
            .await?;
            let credential = credential.expose(|bytes| Credential::new(bytes.to_vec()))?;
            peritus_provider_opencode::discover_models(
                &executable,
                service,
                &credential,
                &CancellationToken::new(),
            )
            .await
        });
    }
    let (endpoint, dialect, name, prefix) = direct_endpoint(kind, endpoint, header)?;
    run(async {
        let endpoint = Endpoint::new(endpoint)?;
        let transport = ReqwestTransport::production()?;
        let headers = || {
            let credential = credential.expose(|bytes| Credential::new(bytes.to_vec()))?;
            let mut headers = vec![credential.into_header(HeaderName::new(name.clone())?, prefix)?];
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

fn direct_endpoint(
    kind: ProviderKind,
    endpoint: Option<&str>,
    header: Option<&str>,
) -> Result<(String, CatalogDialect, String, Option<&'static str>), OnboardingError> {
    Ok(match kind {
        ProviderKind::OpenAiApi => (
            "https://api.openai.com/v1/models".to_owned(),
            CatalogDialect::OpenAi,
            "authorization".to_owned(),
            Some("Bearer "),
        ),
        ProviderKind::AnthropicApi => (
            format!(
                "{}/v1/models?limit=1000",
                endpoint.ok_or(OnboardingError::ModelCatalog)?.trim_end_matches('/')
            ),
            CatalogDialect::Anthropic,
            "x-api-key".to_owned(),
            None,
        ),
        ProviderKind::GoogleGeminiApi => (
            format!(
                "{}/v1beta/models?pageSize=1000",
                endpoint.ok_or(OnboardingError::ModelCatalog)?.trim_end_matches('/')
            ),
            CatalogDialect::Google,
            "x-goog-api-key".to_owned(),
            None,
        ),
        ProviderKind::CompatibleEndpoint => {
            let endpoint = endpoint.ok_or(OnboardingError::ModelCatalog)?;
            let root = endpoint
                .strip_suffix("/responses")
                .or_else(|| endpoint.strip_suffix("/chat/completions"))
                .ok_or(OnboardingError::ModelCatalog)?;
            (
                format!("{root}/models"),
                CatalogDialect::OpenAi,
                header.unwrap_or("authorization").to_owned(),
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
                "authorization".to_owned(),
                Some("Bearer "),
            )
        }
    })
}

pub fn run<T: Send>(
    future: impl Future<Output = Result<T, peritus_provider_core::ProviderCoreError>> + Send,
) -> Result<T, OnboardingError> {
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
