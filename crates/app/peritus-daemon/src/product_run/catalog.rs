//! Provider-advertised catalogs and immutable, explicitly selected role adapters.

use super::{ProductRunService, ProductRunServiceError, interaction::InteractionOptions};
use peritus_app_protocol::{
    ProductModelCatalog, ProductModelChoice, ProductModelEffort, ProductModelInfo,
    ProductModelQuery, ProductProviderSelection,
};
use peritus_model_protocol::ReasoningEffort as Effort;
use peritus_product_runner::RoleProviders;
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::ProviderProfileId;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

impl ProductRunService {
    pub(crate) async fn query_models(
        &self,
        query: ProductModelQuery,
    ) -> Result<ProductModelCatalog, ProductRunServiceError> {
        let provider = self
            .inner
            .providers
            .get(&query.profile())
            .ok_or(ProductRunServiceError::ProviderUnavailable)?;
        // Serialize catalog requests, bounding provider metadata process ownership and request load.
        let mut catalogs = self.inner.model_catalogs.lock().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .as_secs();
        let previous = catalogs.get(&query.profile());
        if !query.refresh()
            && let Some(previous) = previous
            && now.saturating_sub(previous.fetched_unix_seconds()) < 300
        {
            return catalog_copy(previous, true, String::new());
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            provider.discover_models(&CancellationToken::new()),
        )
        .await;
        let catalog = if let Ok(Ok(models)) = result {
            ProductModelCatalog::new(
                query.profile(),
                provider.profile().model().as_str().to_owned(),
                models
                    .into_iter()
                    .map(|model| {
                        ProductModelInfo::new(
                            model.id.as_str().to_owned(),
                            model.label,
                            model.tools,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                now,
                false,
                String::new(),
            )
            .map_err(|_| ProductRunServiceError::InvalidMessage)?
        } else {
            let error = "Model discovery failed for this configured provider. Refresh after checking authentication, or explicitly enter a manual model ID.".to_owned();
            return previous.map_or_else(
                || {
                    ProductModelCatalog::new(
                        query.profile(),
                        provider.profile().model().as_str().to_owned(),
                        Vec::new(),
                        0,
                        false,
                        error.clone(),
                    )
                    .map_err(|_| ProductRunServiceError::InvalidMessage)
                },
                |previous| catalog_copy(previous, true, error.clone()),
            );
        };
        catalogs.insert(query.profile(), catalog.clone());
        Ok(catalog)
    }

    pub(super) async fn validate_models(
        &self,
        providers: ProductProviderSelection,
        options: &InteractionOptions,
    ) -> Result<(), ProductRunServiceError> {
        for (profile, choice) in [
            (providers.writer(), options.models.writer()),
            (providers.reviewer(), options.models.reviewer()),
            (providers.fixer(), options.models.fixer()),
        ] {
            if choice.id().is_empty() || choice.manual() {
                continue;
            }
            let catalog = self.query_models(ProductModelQuery::new(profile, false)).await?;
            if !catalog.models().iter().any(|model| model.id() == choice.id()) {
                return Err(ProductRunServiceError::ProviderUnavailable);
            }
        }
        Ok(())
    }

    pub(super) fn resolve_selected_providers(
        &self,
        selected: ProductProviderSelection,
        options: Option<&InteractionOptions>,
    ) -> Result<RoleProviders, ProductRunServiceError> {
        let Some(options) = options else {
            return self.resolve_providers(selected);
        };
        Ok(RoleProviders {
            writer: self.select_provider(selected.writer(), options.models.writer())?,
            reviewer: self.select_provider(selected.reviewer(), options.models.reviewer())?,
            fixer: self.select_provider(selected.fixer(), options.models.fixer())?,
            // Explicit interactive selections must not silently switch to a different model.
            fallbacks: Vec::new(),
        })
    }

    pub(super) fn select_provider(
        &self,
        profile: ProviderProfileId,
        choice: &ProductModelChoice,
    ) -> Result<Arc<dyn ModelProvider>, ProductRunServiceError> {
        let provider = self
            .inner
            .providers
            .get(&profile)
            .ok_or(ProductRunServiceError::ProviderUnavailable)?;
        let selected = if choice.id().is_empty()
            || choice.id() == provider.profile().model().as_str()
        {
            Arc::clone(provider)
        } else {
            let model = peritus_model_protocol::ModelName::new(choice.id().to_owned())
                .map_err(|_| ProductRunServiceError::InvalidMessage)?;
            provider.select_model(model).map_err(|_| ProductRunServiceError::ProviderUnavailable)?
        };
        let effort = match choice.effort() {
            ProductModelEffort::Default => return Ok(selected),
            ProductModelEffort::Minimal => Effort::Minimal,
            ProductModelEffort::Low => Effort::Low,
            ProductModelEffort::Medium => Effort::Medium,
            ProductModelEffort::High => Effort::High,
            ProductModelEffort::XHigh => Effort::XHigh,
            ProductModelEffort::Max => Effort::Max,
            ProductModelEffort::Ultra => Effort::Ultra,
        };
        peritus_provider_core::select_reasoning_effort(selected, effort)
            .map_err(|_| ProductRunServiceError::EffortUnsupported)
    }
}

fn catalog_copy(
    value: &ProductModelCatalog,
    cached: bool,
    error: String,
) -> Result<ProductModelCatalog, ProductRunServiceError> {
    ProductModelCatalog::new(
        value.profile(),
        value.configured().to_owned(),
        value.models().to_vec(),
        value.fetched_unix_seconds(),
        cached,
        error,
    )
    .map_err(|_| ProductRunServiceError::InvalidMessage)
}
