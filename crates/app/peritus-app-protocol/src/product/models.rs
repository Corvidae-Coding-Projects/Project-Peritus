//! Explicit model choices and provider-catalog observations for interactive clients.

use super::{ProductModelEffort, ProductRunMessageError, bounded_text};
use peritus_types::ProviderProfileId;

/// Maximum model entries returned by one provider catalog.
pub const MAX_PRODUCT_MODELS: usize = 4096;

/// Exact model choice, optionally entered explicitly when discovery is unavailable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductModelChoice {
    id: String,
    manual: bool,
    effort: ProductModelEffort,
}

impl Default for ProductModelChoice {
    fn default() -> Self {
        Self { id: String::new(), manual: false, effort: ProductModelEffort::Default }
    }
}

impl ProductModelChoice {
    /// Selects a discovered model, or an explicitly labeled manual identifier.
    ///
    /// # Errors
    /// Rejects empty, oversized, or control-bearing identifiers.
    pub fn new(id: String, manual: bool) -> Result<Self, ProductRunMessageError> {
        bounded_text(&id, 512)?;
        if id.chars().any(char::is_control) || id.trim() != id {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        Ok(Self { id, manual, effort: ProductModelEffort::Default })
    }
    /// Empty means retain the exact configured provider model, not a built-in fallback.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Whether the user explicitly bypassed catalog membership, not capability enforcement.
    #[must_use]
    pub const fn manual(&self) -> bool {
        self.manual
    }

    /// Requested effort, separate from model identity and catalog provenance.
    #[must_use]
    pub const fn effort(&self) -> ProductModelEffort {
        self.effort
    }

    /// Retains the model selection and sets its requested effort.
    #[must_use]
    pub const fn with_effort(mut self, effort: ProductModelEffort) -> Self {
        self.effort = effort;
        self
    }
}

/// Immutable role model selections carried with an interactive request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProductRoleModels {
    writer: ProductModelChoice,
    reviewer: ProductModelChoice,
    fixer: ProductModelChoice,
}

impl ProductRoleModels {
    /// Whether the selection requires the additive effort-bearing wire payload.
    #[must_use]
    pub fn has_effort(&self) -> bool {
        [self.writer(), self.reviewer(), self.fixer()]
            .iter()
            .any(|choice| choice.effort() != ProductModelEffort::Default)
    }
    /// Creates explicit role selections; defaults retain the configured model.
    #[must_use]
    pub const fn new(
        writer: ProductModelChoice,
        reviewer: ProductModelChoice,
        fixer: ProductModelChoice,
    ) -> Self {
        Self { writer, reviewer, fixer }
    }
    /// Conversational/writer selection.
    #[must_use]
    pub const fn writer(&self) -> &ProductModelChoice {
        &self.writer
    }
    /// Independent reviewer selection.
    #[must_use]
    pub const fn reviewer(&self) -> &ProductModelChoice {
        &self.reviewer
    }
    /// Fixer selection.
    #[must_use]
    pub const fn fixer(&self) -> &ProductModelChoice {
        &self.fixer
    }
}

/// Provider-specific catalog query; refresh never means substitute a fallback list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductModelQuery {
    profile: ProviderProfileId,
    refresh: bool,
}
impl ProductModelQuery {
    /// Queries one configured provider account/endpoint.
    #[must_use]
    pub const fn new(profile: ProviderProfileId, refresh: bool) -> Self {
        Self { profile, refresh }
    }
    /// Exact configured route identity.
    #[must_use]
    pub const fn profile(&self) -> ProviderProfileId {
        self.profile
    }
    /// Force an authenticated query instead of using a fresh cache.
    #[must_use]
    pub const fn refresh(&self) -> bool {
        self.refresh
    }
}

/// One provider-advertised model, not a model-specific conformance certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductModelInfo {
    id: String,
    label: String,
    tools: Option<bool>,
}
impl ProductModelInfo {
    /// Validates the visible provider metadata.
    ///
    /// # Errors
    /// Rejects invalid IDs or labels.
    pub fn new(
        id: String,
        label: String,
        tools: Option<bool>,
    ) -> Result<Self, ProductRunMessageError> {
        let _ = ProductModelChoice::new(id.clone(), false)?;
        bounded_text(&label, 512)?;
        if label.chars().any(char::is_control) {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        Ok(Self { id, label, tools })
    }
    /// Exact provider identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Provider display label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    /// Advertised tool support; absent means unknown, not verified.
    #[must_use]
    pub const fn tools(&self) -> Option<bool> {
        self.tools
    }
}

/// A fresh, cached, or unavailable provider model catalog with explicit provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductModelCatalog {
    profile: ProviderProfileId,
    configured: String,
    models: Vec<ProductModelInfo>,
    fetched_unix_seconds: u64,
    cached: bool,
    error: String,
}
impl ProductModelCatalog {
    /// Validates a complete catalog observation; an error may accompany retained cached models.
    ///
    /// # Errors
    /// Rejects oversized metadata or models without successful-discovery provenance.
    pub fn new(
        profile: ProviderProfileId,
        configured: String,
        models: Vec<ProductModelInfo>,
        fetched_unix_seconds: u64,
        cached: bool,
        error: String,
    ) -> Result<Self, ProductRunMessageError> {
        let _ = ProductModelChoice::new(configured.clone(), false)?;
        if models.len() > MAX_PRODUCT_MODELS
            || error.len() > 4096
            || (!models.is_empty() && fetched_unix_seconds == 0)
        {
            return Err(ProductRunMessageError::TooLong);
        }
        Ok(Self { profile, configured, models, fetched_unix_seconds, cached, error })
    }
    /// Exact configured provider route.
    #[must_use]
    pub const fn profile(&self) -> ProviderProfileId {
        self.profile
    }
    /// Current configured model, separately labeled from discovered entries.
    #[must_use]
    pub fn configured(&self) -> &str {
        &self.configured
    }
    /// Provider-discovered entries only.
    #[must_use]
    pub fn models(&self) -> &[ProductModelInfo] {
        &self.models
    }
    /// Successful fetch timestamp; zero means no successful discovery is available.
    #[must_use]
    pub const fn fetched_unix_seconds(&self) -> u64 {
        self.fetched_unix_seconds
    }
    /// Whether entries came from a previous successful fetch.
    #[must_use]
    pub const fn cached(&self) -> bool {
        self.cached
    }
    /// Safe diagnostic for failed discovery, empty after success.
    #[must_use]
    pub fn error(&self) -> &str {
        &self.error
    }
}
