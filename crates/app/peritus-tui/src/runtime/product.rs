//! Launcher-resolved product facts displayed by the interactive client.

use peritus_types::{ProviderProfileId, WorkspaceId};

use crate::TuiError;

/// One enabled provider choice shown in the coding-run composer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductProviderOption {
    profile_id: ProviderProfileId,
    label: String,
}

impl ProductProviderOption {
    /// Creates a displayed provider option.
    #[must_use]
    pub fn new(profile_id: ProviderProfileId, label: impl Into<String>) -> Self {
        Self { profile_id, label: label.into() }
    }

    /// Exact provider-profile identity.
    #[must_use]
    pub const fn profile_id(&self) -> ProviderProfileId {
        self.profile_id
    }

    /// Human-facing provider name.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Product facts already selected and trusted by the launcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductLaunchContext {
    workspace_id: WorkspaceId,
    workspace_label: String,
    providers: Vec<ProductProviderOption>,
    default_provider: Option<usize>,
}

impl ProductLaunchContext {
    /// Creates a launch context with a valid displayed default.
    ///
    /// # Errors
    ///
    /// Rejects an empty provider list or out-of-range default.
    pub fn new(
        workspace_id: WorkspaceId,
        workspace_label: String,
        providers: Vec<ProductProviderOption>,
        default_provider: Option<usize>,
    ) -> Result<Self, TuiError> {
        if default_provider.is_some_and(|index| index >= providers.len())
            || providers.is_empty() != default_provider.is_none()
        {
            return Err(TuiError::InvalidValue(
                "product provider default does not match the enabled provider list".to_owned(),
            ));
        }
        Ok(Self { workspace_id, workspace_label, providers, default_provider })
    }

    /// Active workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Active workspace label.
    #[must_use]
    pub fn workspace_label(&self) -> &str {
        &self.workspace_label
    }

    /// Enabled provider choices.
    #[must_use]
    pub fn providers(&self) -> &[ProductProviderOption] {
        &self.providers
    }

    /// Default provider index.
    #[must_use]
    pub const fn default_provider(&self) -> Option<usize> {
        self.default_provider
    }
}
