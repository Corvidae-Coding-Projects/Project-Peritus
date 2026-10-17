//! Optional durable reviewer provider/model override for new conversations.

use serde::{Deserialize, Serialize};

use super::{ProductStateError, ProviderKind, ProviderSelection, bounded_text};

/// Explicit reviewer choice, independent of the ordinary coding model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerSelection {
    provider: ProviderKind,
    model: String,
    manual: bool,
}

impl ReviewerSelection {
    /// Records a discovered or explicitly entered reviewer model.
    ///
    /// # Errors
    /// Rejects empty, oversized, whitespace-bearing, or control-bearing model identifiers.
    pub fn new(
        provider: ProviderKind,
        model: String,
        manual: bool,
    ) -> Result<Self, ProductStateError> {
        let selection = Self { provider, model, manual };
        selection.validate()?;
        Ok(selection)
    }

    /// Configured provider that supplies the reviewer model.
    #[must_use]
    pub const fn provider(&self) -> ProviderKind {
        self.provider
    }

    /// Exact selected reviewer model identifier.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Whether the user explicitly bypassed model discovery.
    #[must_use]
    pub const fn manual(&self) -> bool {
        self.manual
    }

    fn validate(&self) -> Result<(), ProductStateError> {
        if !bounded_text(&self.model, 256)
            || self
                .model
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(ProductStateError::InvalidPayload("reviewer model is invalid".to_owned()));
        }
        Ok(())
    }
}

impl ProviderSelection {
    /// Sets an optional reviewer override; none follows the ordinary default provider/model.
    ///
    /// # Errors
    /// Rejects a disabled provider or malformed model choice.
    pub fn with_reviewer(
        mut self,
        reviewer: Option<ReviewerSelection>,
    ) -> Result<Self, ProductStateError> {
        if let Some(choice) = &reviewer {
            choice.validate()?;
            if !self.enabled.contains(&choice.provider) {
                return Err(ProductStateError::InvalidPayload(
                    "reviewer provider must be enabled".to_owned(),
                ));
            }
        }
        self.reviewer = reviewer;
        Ok(self)
    }

    /// Optional explicit reviewer choice for new conversations.
    #[must_use]
    pub const fn reviewer(&self) -> Option<&ReviewerSelection> {
        self.reviewer.as_ref()
    }
}
