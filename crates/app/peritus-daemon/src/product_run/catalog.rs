//! Provider-advertised model catalogs and immutable, explicitly selected role adapters.

mod runs;

pub(super) use runs::{RunCatalog, migrate_sequences};

use super::{
    ProductRunService, ProductRunServiceError, RunRecord, interaction::InteractionOptions,
};
use peritus_app_protocol::{
    ProductModelCatalog, ProductModelChoice, ProductModelEffort, ProductModelInfo,
    ProductModelQuery, ProductProviderSelection,
};
use peritus_model_protocol::ReasoningEffort as Effort;
use peritus_product_runner::RoleProviders;
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::{ProviderProfileId, RunId};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub(super) struct ModelCatalogs {
    models: tokio::sync::Mutex<BTreeMap<ProviderProfileId, Arc<CatalogSlot>>>,
    runs: std::sync::RwLock<RunCatalog>,
}

#[derive(Default)]
struct CatalogSlot {
    latest: tokio::sync::RwLock<Option<ProductModelCatalog>>,
    discovery: tokio::sync::Mutex<()>,
}

impl ModelCatalogs {
    pub(super) fn with_runs(
        records: &BTreeMap<RunId, RunRecord>,
        frontier: u64,
    ) -> Result<Self, ProductRunServiceError> {
        Ok(Self {
            models: tokio::sync::Mutex::new(BTreeMap::new()),
            runs: std::sync::RwLock::new(RunCatalog::from_records(records, frontier)?),
        })
    }

    async fn slot(&self, profile: ProviderProfileId) -> Arc<CatalogSlot> {
        Arc::clone(self.models.lock().await.entry(profile).or_default())
    }

    pub(super) fn runs(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, RunCatalog>, ProductRunServiceError> {
        self.runs.read().map_err(|_| ProductRunServiceError::Unavailable)
    }

    pub(super) fn runs_mut(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, RunCatalog>, ProductRunServiceError> {
        self.runs.write().map_err(|_| ProductRunServiceError::Unavailable)
    }
}

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
        let slot = self.inner.model_catalogs.slot(query.profile()).await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .as_secs();
        let previous = slot.latest.read().await.clone();
        if !query.refresh()
            && let Some(previous) = &previous
            && now.saturating_sub(previous.fetched_unix_seconds()) < 300
        {
            return catalog_copy(previous, true, String::new());
        }
        // One lookup per provider; its cache and unrelated routes remain readable. Repeated
        // refreshes do not queue remote processes behind a stalled metadata request.
        let Ok(_discovery) = slot.discovery.try_lock() else {
            return unavailable_catalog(
                query.profile(),
                provider.profile().model().as_str(),
                previous.as_ref(),
                "Model discovery is already running for this provider. Refresh to retry; any listed models are cached.",
            );
        };
        let result = provider.discover_models(&CancellationToken::new()).await;
        let catalog = if let Ok(models) = result {
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
            return unavailable_catalog(
                query.profile(),
                provider.profile().model().as_str(),
                previous.as_ref(),
                "Model discovery failed for this configured provider. Refresh after checking authentication, or explicitly enter a manual model ID.",
            );
        };
        *slot.latest.write().await = Some(catalog.clone());
        Ok(catalog)
    }

    pub(super) async fn validate_models(
        &self,
        providers: ProductProviderSelection,
        models: &peritus_app_protocol::ProductRoleModels,
    ) -> Result<(), ProductRunServiceError> {
        for (profile, choice) in [
            (providers.writer(), models.writer()),
            (providers.reviewer(), models.reviewer()),
            (providers.fixer(), models.fixer()),
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
        options: &InteractionOptions,
    ) -> Result<RoleProviders, ProductRunServiceError> {
        Ok(RoleProviders {
            writer: self.select_provider(selected.writer(), options.models.writer())?,
            reviewer: self.select_provider(selected.reviewer(), options.models.reviewer())?,
            fixer: self.select_provider(selected.fixer(), options.models.fixer())?,
            // Explicit interactive selections must not silently switch to a different model.
            fallbacks: Vec::new(),
        })
    }

    /// Validates every immediately usable role and reports whether another role is retained
    /// pending exact facts for its selected model.
    pub(super) fn validate_model_resolution(
        &self,
        selected: ProductProviderSelection,
        models: &peritus_app_protocol::ProductRoleModels,
    ) -> Result<bool, ProductRunServiceError> {
        let mut pending = false;
        for (profile, choice) in [
            (selected.writer(), models.writer()),
            (selected.reviewer(), models.reviewer()),
            (selected.fixer(), models.fixer()),
        ] {
            let provider = self
                .inner
                .providers
                .get(&profile)
                .ok_or(ProductRunServiceError::ProviderUnavailable)?;
            if !choice.id().is_empty()
                && choice.id() != provider.profile().model().as_str()
            {
                pending = true;
            } else {
                let _ = self.select_provider(profile, choice)?;
            }
        }
        Ok(pending)
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
            return Err(ProductRunServiceError::ModelFactsRequired);
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

fn unavailable_catalog(
    profile: ProviderProfileId,
    configured: &str,
    previous: Option<&ProductModelCatalog>,
    error: &str,
) -> Result<ProductModelCatalog, ProductRunServiceError> {
    if let Some(previous) = previous {
        return catalog_copy(previous, true, error.to_owned());
    }
    ProductModelCatalog::new(profile, configured.to_owned(), Vec::new(), 0, false, error.to_owned())
        .map_err(|_| ProductRunServiceError::InvalidMessage)
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
