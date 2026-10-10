//! Durable account model selections and resolved executable pins.

use super::{ProductStateError, ProviderKind, ProviderSelection, bounded_text};

impl ProviderSelection {
    /// Retains resolved executable paths as non-secret configuration facts.
    ///
    /// # Errors
    /// Rejects disabled/non-account routes, relative paths, controls, and oversized paths.
    pub fn with_account_executables(
        mut self,
        executables: std::collections::BTreeMap<ProviderKind, String>,
    ) -> Result<Self, ProductStateError> {
        if executables.iter().any(|(kind, path)| {
            !kind.is_account()
                || !self.enabled.contains(kind)
                || !bounded_text(path, 32_768)
                || path.chars().any(char::is_control)
                || !std::path::Path::new(path).is_absolute()
        }) {
            return Err(ProductStateError::InvalidPayload(
                "account executable selection is invalid".to_owned(),
            ));
        }
        self.account_executables = executables;
        Ok(self)
    }

    /// Borrows the exact executable pinned in this durable provider selection.
    #[must_use]
    pub fn account_executable(&self, kind: ProviderKind) -> Option<&str> {
        self.account_executables.get(&kind).map(String::as_str)
    }

    /// Retains exact account model selections; old records may omit them until migration.
    ///
    /// # Errors
    /// Rejects disabled/non-account routes and empty or unsafe identifiers.
    pub fn with_account_models(
        mut self,
        models: std::collections::BTreeMap<ProviderKind, String>,
    ) -> Result<Self, ProductStateError> {
        if models.iter().any(|(kind, model)| {
            !kind.is_account()
                || !self.enabled.contains(kind)
                || !bounded_text(model, 256)
                || model.chars().any(char::is_control)
        }) {
            return Err(ProductStateError::InvalidPayload(
                "account model selection is invalid".to_owned(),
            ));
        }
        self.account_models = models;
        Ok(self)
    }

    /// Exact selected account model, never a built-in provider default.
    #[must_use]
    pub fn account_model(&self, kind: ProviderKind) -> Option<&str> {
        self.account_models.get(&kind).map(String::as_str)
    }
}
