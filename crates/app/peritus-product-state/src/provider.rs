//! Durable provider choices without credentials or live-status claims.

use serde::Deserialize;
use serde::Serialize;

use crate::ProductStateError;

/// Provider login routes selectable in the product.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// Subscription-backed `OpenAI` access through the official Codex executable.
    CodexAccount,
    /// Subscription-backed Anthropic access through the official Claude executable.
    ClaudeAccount,
    /// Direct `OpenAI` API access through an operating-system credential store.
    OpenAiApi,
    /// Direct Anthropic API access through an operating-system credential store.
    AnthropicApi,
    /// Direct Google Gemini API access through an operating-system credential store.
    GoogleGeminiApi,
    /// Explicit compatible HTTP endpoint with credential-store-backed authentication.
    CompatibleEndpoint,
}

impl ProviderKind {
    /// Returns the user-facing provider label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CodexAccount => "OpenAI with ChatGPT account",
            Self::ClaudeAccount => "Anthropic with Claude account",
            Self::OpenAiApi => "OpenAI API",
            Self::AnthropicApi => "Anthropic API",
            Self::GoogleGeminiApi => "Google Gemini API",
            Self::CompatibleEndpoint => "Compatible endpoint",
        }
    }

    /// Returns whether the route delegates account ownership to an official executable.
    #[must_use]
    pub const fn is_account(self) -> bool {
        matches!(self, Self::CodexAccount | Self::ClaudeAccount)
    }

    /// Returns whether the route uses a credential stored by the operating system.
    #[must_use]
    pub const fn is_direct(self) -> bool {
        !self.is_account()
    }
}

/// Wire family selected for an explicitly compatible endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatibleProtocol {
    /// `OpenAI` Responses-compatible request and stream shapes.
    Responses,
    /// `OpenAI` Chat Completions-compatible request and stream shapes.
    ChatCompletions,
}

/// Durable non-secret configuration for one direct provider route.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectProviderProfile {
    kind: ProviderKind,
    credential_reference: String,
    endpoint: Option<String>,
    model: String,
    compatible_protocol: Option<CompatibleProtocol>,
    credential_header: Option<String>,
}

impl DirectProviderProfile {
    /// Creates one bounded direct route containing only an opaque credential reference.
    ///
    /// # Errors
    ///
    /// Rejects account routes, missing required fields, misplaced compatible fields, or unsafe
    /// text bounds. Exact endpoint and model semantics are validated by the production adapter.
    pub fn new(
        kind: ProviderKind,
        credential_reference: String,
        endpoint: Option<String>,
        model: String,
        compatible_protocol: Option<CompatibleProtocol>,
        credential_header: Option<String>,
    ) -> Result<Self, ProductStateError> {
        let profile = Self {
            kind,
            credential_reference,
            endpoint,
            model,
            compatible_protocol,
            credential_header,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Returns the direct provider kind.
    #[must_use]
    pub const fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Borrows the opaque credential reference.
    #[must_use]
    pub fn credential_reference(&self) -> &str {
        &self.credential_reference
    }

    /// Borrows the configured endpoint when required by the adapter.
    #[must_use]
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    /// Borrows the selected provider model.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Returns the compatible wire family when this is a compatible endpoint.
    #[must_use]
    pub const fn compatible_protocol(&self) -> Option<CompatibleProtocol> {
        self.compatible_protocol
    }

    /// Borrows an optional compatible raw credential header.
    #[must_use]
    pub fn credential_header(&self) -> Option<&str> {
        self.credential_header.as_deref()
    }

    fn validate(&self) -> Result<(), ProductStateError> {
        let endpoint_required = matches!(
            self.kind,
            ProviderKind::AnthropicApi
                | ProviderKind::GoogleGeminiApi
                | ProviderKind::CompatibleEndpoint
        );
        let compatible = self.kind == ProviderKind::CompatibleEndpoint;
        if !self.kind.is_direct()
            || endpoint_required != self.endpoint.is_some()
            || compatible != self.compatible_protocol.is_some()
            || !compatible && self.credential_header.is_some()
            || !bounded_text(&self.model, 256)
            || !bounded_text(&self.credential_reference, 256)
            || !self.credential_reference.starts_with("peritus-secret-v1:")
            || self.endpoint.as_deref().is_some_and(|value| !bounded_text(value, 2_048))
            || self.credential_header.as_deref().is_some_and(|value| !bounded_text(value, 128))
        {
            return Err(ProductStateError::InvalidPayload(
                "direct provider profile is malformed or exceeds its bounds".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Canonical enabled-provider set and optional default.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSelection {
    enabled: Vec<ProviderKind>,
    default: Option<ProviderKind>,
    #[serde(default)]
    automatic_failover: bool,
    #[serde(default)]
    direct_profiles: Vec<DirectProviderProfile>,
}

impl ProviderSelection {
    /// Validates, sorts, and stores selected providers.
    ///
    /// An empty selection is the explicit offline-browse mode and has no default.
    ///
    /// # Errors
    ///
    /// Returns invalid payload when a default is not enabled.
    pub fn new(
        mut enabled: Vec<ProviderKind>,
        default: Option<ProviderKind>,
    ) -> Result<Self, ProductStateError> {
        enabled.sort_unstable();
        enabled.dedup();
        if default.is_some_and(|kind| !enabled.contains(&kind))
            || enabled.is_empty() && default.is_some()
        {
            return Err(ProductStateError::InvalidPayload(
                "default provider must belong to the enabled provider set".to_owned(),
            ));
        }
        Self::with_direct_profiles(enabled, default, Vec::new())
    }

    /// Validates and stores selected providers plus non-secret direct-route profiles.
    ///
    /// # Errors
    ///
    /// Returns invalid payload unless every enabled direct route has exactly one matching profile.
    pub fn with_direct_profiles(
        enabled: Vec<ProviderKind>,
        default: Option<ProviderKind>,
        direct_profiles: Vec<DirectProviderProfile>,
    ) -> Result<Self, ProductStateError> {
        Self::with_direct_profiles_and_failover(enabled, default, direct_profiles, false)
    }

    /// Validates and stores provider routes plus explicit automatic-failover consent.
    ///
    /// # Errors
    ///
    /// Returns invalid payload unless every direct route is complete and failover has at least
    /// two enabled provider choices.
    pub fn with_direct_profiles_and_failover(
        mut enabled: Vec<ProviderKind>,
        default: Option<ProviderKind>,
        mut direct_profiles: Vec<DirectProviderProfile>,
        automatic_failover: bool,
    ) -> Result<Self, ProductStateError> {
        enabled.sort_unstable();
        enabled.dedup();
        direct_profiles.sort_unstable();
        let enabled_count = u64::try_from(enabled.len()).unwrap_or(u64::MAX);
        if default.is_some_and(|kind| !enabled.contains(&kind))
            || enabled.is_empty() && default.is_some()
            || !crate::verified::provider_failover_shape_exec(enabled_count, automatic_failover)
            || direct_profiles.windows(2).any(|pair| pair[0].kind == pair[1].kind)
            || direct_profiles
                .iter()
                .any(|profile| profile.validate().is_err() || !enabled.contains(&profile.kind))
            || enabled
                .iter()
                .filter(|kind| kind.is_direct())
                .any(|kind| !direct_profiles.iter().any(|profile| profile.kind == *kind))
        {
            return Err(ProductStateError::InvalidPayload(
                "provider selection and direct profiles do not match".to_owned(),
            ));
        }
        Ok(Self { enabled, default, automatic_failover, direct_profiles })
    }

    /// Borrows the canonical enabled providers.
    #[must_use]
    pub fn enabled(&self) -> &[ProviderKind] {
        &self.enabled
    }

    /// Returns the default provider, when model-backed runs are enabled.
    #[must_use]
    pub const fn default(&self) -> Option<ProviderKind> {
        self.default
    }

    /// Returns whether the user allowed a role to switch after its selected provider exhausts
    /// ordinary recovery.
    #[must_use]
    pub const fn automatic_failover(&self) -> bool {
        self.automatic_failover
    }

    /// Borrows canonical direct-route profiles.
    #[must_use]
    pub fn direct_profiles(&self) -> &[DirectProviderProfile] {
        &self.direct_profiles
    }

    /// Borrows one direct-route profile by provider kind.
    #[must_use]
    pub fn direct_profile(&self, kind: ProviderKind) -> Option<&DirectProviderProfile> {
        self.direct_profiles.iter().find(|profile| profile.kind == kind)
    }

    /// Returns whether setup has an enabled provider.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        !self.enabled.is_empty()
    }

    pub(crate) fn validate(&self) -> Result<(), ProductStateError> {
        let canonical = Self::with_direct_profiles_and_failover(
            self.enabled.clone(),
            self.default,
            self.direct_profiles.clone(),
            self.automatic_failover,
        )?;
        if &canonical != self {
            return Err(ProductStateError::InvalidPayload(
                "enabled providers are not canonical".to_owned(),
            ));
        }
        Ok(())
    }
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.bytes().all(|byte| !byte.is_ascii_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failover_is_explicit_and_requires_two_routes() {
        let primary = ProviderSelection::new(
            vec![ProviderKind::CodexAccount, ProviderKind::ClaudeAccount],
            Some(ProviderKind::CodexAccount),
        )
        .expect("primary-only selection");
        assert!(!primary.automatic_failover());

        let failover = ProviderSelection::with_direct_profiles_and_failover(
            vec![ProviderKind::CodexAccount, ProviderKind::ClaudeAccount],
            Some(ProviderKind::CodexAccount),
            Vec::new(),
            true,
        )
        .expect("explicit failover selection");
        assert!(failover.automatic_failover());
        assert!(
            ProviderSelection::with_direct_profiles_and_failover(
                vec![ProviderKind::CodexAccount],
                Some(ProviderKind::CodexAccount),
                Vec::new(),
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn old_state_shape_defaults_failover_off() {
        let selection: ProviderSelection = serde_json::from_str(
            r#"{"enabled":["codex-account"],"default":"codex-account","direct_profiles":[]}"#,
        )
        .expect("old selection shape");
        assert!(!selection.automatic_failover());
        selection.validate().expect("old selection remains valid");
    }
}
