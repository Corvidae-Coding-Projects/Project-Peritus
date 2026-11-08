//! Explicit defaults for all execution roles, separate from provider credentials.
use super::{ProviderKind, ProviderSelection};
use crate::ProductStateError;
use serde::{Deserialize, Serialize};

/// User-requested reasoning policy; off is explicit, never an inferred effort.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleEffort {
    /// No reasoning control on a route that does not support it.
    Off,
    /// Minimal reasoning.
    Minimal,
    /// Low reasoning.
    Low,
    /// Medium reasoning.
    Medium,
    /// High reasoning.
    High,
    /// Extra-high reasoning where supported.
    #[serde(rename = "xhigh")]
    XHigh,
    /// Maximum reasoning where supported.
    Max,
    /// Ultra reasoning where supported.
    Ultra,
}
impl RoleEffort {
    /// Supported settings vocabulary, subject to the selected model's actual support.
    pub const ALL: [Self; 8] = [
        Self::Off,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
        Self::Ultra,
    ];
    /// Stable command spelling.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }
    /// Parses an explicit setting without substituting another policy.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.label() == value)
    }
}

/// One exact, explicitly selected role default.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleModelSelection {
    provider: ProviderKind,
    model: String,
    manual: bool,
    effort: RoleEffort,
}
impl RoleModelSelection {
    /// Records an exact role choice.
    ///
    /// # Errors
    /// Rejects malformed model identifiers.
    pub fn new(
        provider: ProviderKind,
        model: String,
        manual: bool,
        effort: RoleEffort,
    ) -> Result<Self, ProductStateError> {
        let value = Self { provider, model, manual, effort };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), ProductStateError> {
        if self.provider.is_account() && self.effort == RoleEffort::Off {
            return Err(ProductStateError::InvalidPayload(
                "account role reasoning effort must be selected".into(),
            ));
        }
        if self.model.is_empty()
            || self.model.len() > 512
            || self.model.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(ProductStateError::InvalidPayload("role model is invalid".into()));
        }
        Ok(())
    }
    /// Configured provider route.
    #[must_use]
    pub const fn provider(&self) -> ProviderKind {
        self.provider
    }
    /// Exact model identifier.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
    /// Explicit catalog bypass, retaining ordinary support checks.
    #[must_use]
    pub const fn manual(&self) -> bool {
        self.manual
    }
    /// Requested reasoning policy.
    #[must_use]
    pub const fn effort(&self) -> RoleEffort {
        self.effort
    }
}

/// Complete startup defaults; partial legacy state remains absent and needs setup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleModelDefaults {
    writer: RoleModelSelection,
    reviewer: RoleModelSelection,
    fixer: RoleModelSelection,
}
impl RoleModelDefaults {
    /// Groups three independently chosen roles; explicit copies are ordinary choices.
    #[must_use]
    pub const fn new(
        writer: RoleModelSelection,
        reviewer: RoleModelSelection,
        fixer: RoleModelSelection,
    ) -> Self {
        Self { writer, reviewer, fixer }
    }
    /// Coding role.
    #[must_use]
    pub const fn writer(&self) -> &RoleModelSelection {
        &self.writer
    }
    /// Independent review role.
    #[must_use]
    pub const fn reviewer(&self) -> &RoleModelSelection {
        &self.reviewer
    }
    /// Remediation role.
    #[must_use]
    pub const fn fixer(&self) -> &RoleModelSelection {
        &self.fixer
    }
    /// Checks that all routes still belong to the enabled provider set.
    #[must_use]
    pub fn providers_enabled(&self, selection: &ProviderSelection) -> bool {
        [self.writer(), self.reviewer(), self.fixer()]
            .iter()
            .all(|r| selection.enabled().contains(&r.provider()))
    }
}
impl ProviderSelection {
    /// Sets explicit defaults without changing provider credentials or existing conversations.
    ///
    /// # Errors
    /// Rejects disabled providers or invalid role models.
    pub fn with_role_models(
        mut self,
        roles: Option<RoleModelDefaults>,
    ) -> Result<Self, ProductStateError> {
        if let Some(roles) = &roles {
            for choice in [roles.writer(), roles.reviewer(), roles.fixer()] {
                choice.validate()?;
            }
            if !roles.providers_enabled(&self) {
                return Err(ProductStateError::InvalidPayload(
                    "role provider must be enabled".into(),
                ));
            }
        }
        self.role_models = roles;
        Ok(self)
    }
    /// Complete startup defaults, absent on legacy installations until explicitly selected.
    #[must_use]
    pub const fn role_models(&self) -> Option<&RoleModelDefaults> {
        self.role_models.as_ref()
    }
}
