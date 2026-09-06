//! Model discovery beside the configured inference route, preserving custom authentication.

use super::CompatibleClient;
use peritus_provider_core::{
    CancellationToken, Endpoint, HttpHeaders, ProviderCoreError,
    catalog::{CatalogDialect, DiscoveredModel, discover_http_models, unavailable},
};

impl CompatibleClient {
    pub(super) async fn catalog(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
        let source = self.config.endpoint().as_str();
        let base = source.strip_suffix("/responses").or_else(|| source.strip_suffix("/chat/completions"))
            .ok_or_else(|| unavailable("cannot derive a models route from this custom inference path; select an explicit model ID"))?;
        let endpoint = Endpoint::new(format!("{base}/models"))?;
        discover_http_models(
            self.transport.as_ref(),
            &endpoint,
            CatalogDialect::OpenAi,
            &|| {
                let mut headers = vec![
                    self.config
                        .auth()
                        .project(self.credentials.resolve(self.config.auth().credential())?)?,
                ];
                for header in self.config.fixed_headers() {
                    headers.push(header.project()?);
                }
                HttpHeaders::new(headers, self.config.http_limits())
            },
            self.config.http_limits(),
            cancellation,
        )
        .await
    }
}
