//! Durable account model selections and resolved executable pins.

use super::{
    ProductStateError, ProviderKind, ProviderModelCapability, ProviderModelFacts,
    ProviderSelection, bounded_text,
};

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
        self.account_model_facts.retain(|kind, facts| {
            self.account_models.get(kind).is_some_and(|model| model == facts.model())
        });
        Ok(self)
    }

    /// Exact selected account model, never a built-in provider default.
    #[must_use]
    pub fn account_model(&self, kind: ProviderKind) -> Option<&str> {
        self.account_models.get(&kind).map(String::as_str)
    }

    /// Retains versioned facts for exact selected account models.
    ///
    /// # Errors
    /// Rejects facts for disabled routes, missing model selections, or different model IDs.
    pub fn with_account_model_facts(
        mut self,
        facts: std::collections::BTreeMap<ProviderKind, ProviderModelFacts>,
    ) -> Result<Self, ProductStateError> {
        if facts.iter().any(|(kind, facts)| {
            !kind.is_account()
                || !self.enabled.contains(kind)
                || facts.validate().is_err()
                || self.account_models.get(kind).map(String::as_str) != Some(facts.model())
                || facts.capabilities().iter().any(|capability| {
                    !matches!(
                        (*kind, *capability),
                        (
                            ProviderKind::CodexAccount,
                            ProviderModelCapability::ToolCalls
                                | ProviderModelCapability::ParallelToolCalls
                                | ProviderModelCapability::PromptCaching
                                | ProviderModelCapability::ImageInput
                                | ProviderModelCapability::ReasoningControls
                                | ProviderModelCapability::UsageDetail
                        ) | (
                            ProviderKind::ClaudeAccount,
                            ProviderModelCapability::ToolCalls
                                | ProviderModelCapability::ParallelToolCalls
                                | ProviderModelCapability::PromptCaching
                                | ProviderModelCapability::ReasoningControls
                                | ProviderModelCapability::UsageDetail
                        )
                    )
                })
        }) {
            return Err(ProductStateError::InvalidPayload(
                "account model facts do not match the exact selected models".to_owned(),
            ));
        }
        self.account_model_facts = facts;
        Ok(self)
    }

    /// Exact facts for one selected account model; absence means legacy or unresolved capacity.
    #[must_use]
    pub fn account_model_facts(&self, kind: ProviderKind) -> Option<&ProviderModelFacts> {
        self.account_model_facts.get(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn executable_pins_round_trip_and_legacy_selection_remains_valid() {
        let path = std::env::current_exe().expect("executable").to_string_lossy().into_owned();
        let selection = ProviderSelection::new(
            vec![ProviderKind::CodexAccount],
            Some(ProviderKind::CodexAccount),
        )
        .expect("selection")
        .with_account_executables(BTreeMap::from([(ProviderKind::CodexAccount, path.clone())]))
        .expect("pin");
        let mut json = serde_json::to_value(&selection).expect("serialize");
        let decoded: ProviderSelection = serde_json::from_value(json.clone()).expect("round trip");
        decoded.validate().expect("valid pin");
        assert_eq!(decoded.account_executable(ProviderKind::CodexAccount), Some(path.as_str()));
        json.as_object_mut().expect("object").remove("account_executables");
        let legacy: ProviderSelection = serde_json::from_value(json).expect("legacy selection");
        legacy.validate().expect("legacy still valid");
        assert!(legacy.account_executable(ProviderKind::CodexAccount).is_none());
    }

    #[test]
    fn invalid_or_unselected_executables_are_rejected() {
        let selection = ProviderSelection::new(
            vec![ProviderKind::CodexAccount],
            Some(ProviderKind::CodexAccount),
        )
        .expect("selection");
        for path in
            [String::new(), "relative/client".to_owned(), "\n".to_owned(), "x".repeat(32_769)]
        {
            assert!(
                selection
                    .clone()
                    .with_account_executables(BTreeMap::from([(ProviderKind::CodexAccount, path)]))
                    .is_err()
            );
        }
        let path = std::env::current_exe().expect("executable").to_string_lossy().into_owned();
        assert!(
            selection
                .with_account_executables(BTreeMap::from([(ProviderKind::ClaudeAccount, path)]))
                .is_err()
        );
    }
}
