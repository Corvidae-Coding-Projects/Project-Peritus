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
    }
}

pub fn direct(
    kind: ProviderKind,
    endpoint: Option<&str>,
    header: Option<&str>,
    credential: &peritus_secrets::SecretMaterial,
) -> Result<Vec<String>, OnboardingError> {
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
                "{}/v1beta/models?pageSize=1000",
                endpoint.ok_or(OnboardingError::ModelCatalog)?.trim_end_matches('/')
            ),
            CatalogDialect::Google,
            "x-goog-api-key",
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
                header.unwrap_or("authorization"),
                if header.is_none() { Some("Bearer ") } else { None },
            )
        }
        _ => return Err(OnboardingError::UnsupportedProvider),
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

fn run(
    future: impl Future<Output = Result<Vec<DiscoveredModel>, peritus_provider_core::ProviderCoreError>>
    + Send,
) -> Result<Vec<String>, OnboardingError> {
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| OnboardingError::ModelCatalog)?;
                runtime
                    .block_on(future)
                    .map(|models| {
                        models.into_iter().map(|model| model.id.as_str().to_owned()).collect()
                    })
                    .map_err(|_| OnboardingError::ModelCatalog)
            })
            .join()
            .map_err(|_| OnboardingError::ModelCatalog)?
    })
}
