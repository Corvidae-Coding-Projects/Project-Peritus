//! Authenticated model discovery on the configured first-party route.

use super::OpenAiProvider;
use peritus_provider_core::{
    CancellationToken, Header, HeaderName, HttpHeaders, ProviderCoreError,
    catalog::{CatalogDialect, DiscoveredModel, discover_http_models},
};

impl OpenAiProvider {
    pub(super) async fn catalog(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
        let endpoint = self.config.endpoint().with_path("/v1/models")?;
        discover_http_models(
            self.transport.as_ref(),
            &endpoint,
            CatalogDialect::OpenAi,
            &|| {
                let credential = self.credentials.resolve(self.config.credential())?;
                let mut headers =
                    vec![credential.into_header(
                        HeaderName::new("authorization".to_owned())?,
                        Some("Bearer "),
                    )?];
                if let Some(value) = self.config.organization() {
                    headers.push(Header::new(
                        HeaderName::new("openai-organization".to_owned())?,
                        value.to_vec(),
                    )?);
                }
                if let Some(value) = self.config.project() {
                    headers.push(Header::new(
                        HeaderName::new("openai-project".to_owned())?,
                        value.to_vec(),
                    )?);
                }
                HttpHeaders::new(headers, self.config.http_limits())
            },
            self.config.http_limits(),
            cancellation,
        )
        .await
    }
}
