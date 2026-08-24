//! Immutable endpoint, credential, profile, beta, and resource configuration.

use core::fmt;

use peritus_model_protocol::ProviderProfile;
use peritus_provider_core::{
    CredentialReference, Endpoint, FramingLimits, HttpLimits, ProviderCoreError, RetryPolicy,
};

use crate::profile::validate_anthropic_profile;

/// One explicitly supported Anthropic beta header value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AnthropicBeta {
    /// Prompt caching beta contract dated 2024-07-31.
    PromptCaching20240731,
    /// Files API beta contract dated 2025-04-14.
    FilesApi20250414,
    /// One-million-token context beta contract dated 2025-08-07.
    Context1m20250807,
    /// Structured outputs beta contract dated 2025-11-13.
    StructuredOutputs20251113,
}

impl AnthropicBeta {
    pub(crate) const fn header_value(self) -> &'static str {
        match self {
            Self::PromptCaching20240731 => "prompt-caching-2024-07-31",
            Self::FilesApi20250414 => "files-api-2025-04-14",
            Self::Context1m20250807 => "context-1m-2025-08-07",
            Self::StructuredOutputs20251113 => "structured-outputs-2025-11-13",
        }
    }
}

/// Complete immutable configuration for one Anthropic Messages adapter instance.
#[derive(Clone)]
pub struct AnthropicConfig {
    endpoint: Endpoint,
    credential: CredentialReference,
    profile: ProviderProfile,
    betas: Vec<AnthropicBeta>,
    http_limits: HttpLimits,
    framing_limits: FramingLimits,
    retry_policy: RetryPolicy,
}

impl AnthropicConfig {
    /// Creates a profile-bound configuration and canonicalizes beta header order.
    ///
    /// # Errors
    ///
    /// Rejects a non-Anthropic profile, lifecycle drift, duplicate betas, or more than eight beta
    /// contracts. Endpoint and credential references are already checked core values.
    pub fn new(
        endpoint: Endpoint,
        credential: CredentialReference,
        profile: ProviderProfile,
        mut betas: Vec<AnthropicBeta>,
        http_limits: HttpLimits,
        framing_limits: FramingLimits,
        retry_policy: RetryPolicy,
    ) -> Result<Self, ProviderCoreError> {
        validate_anthropic_profile(&profile)?;
        betas.sort_unstable();
        if betas.len() > 8 || betas.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ProviderCoreError::configuration(
                "anthropic_config",
                "Anthropic beta configuration is duplicate or exceeds its count bound",
            ));
        }
        Ok(Self { endpoint, credential, profile, betas, http_limits, framing_limits, retry_policy })
    }

    /// Returns the exact immutable provider profile.
    #[must_use]
    pub const fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    /// Returns the configured endpoint without appending the Messages path.
    #[must_use]
    pub const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Returns explicitly enabled beta contracts in canonical header order.
    #[must_use]
    pub fn betas(&self) -> &[AnthropicBeta] {
        &self.betas
    }

    pub(crate) const fn credential(&self) -> &CredentialReference {
        &self.credential
    }

    pub(crate) const fn http_limits(&self) -> HttpLimits {
        self.http_limits
    }

    pub(crate) const fn framing_limits(&self) -> FramingLimits {
        self.framing_limits
    }

    pub(crate) const fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy
    }

    pub(crate) fn beta_header(&self) -> Option<Vec<u8>> {
        (!self.betas.is_empty()).then(|| {
            self.betas
                .iter()
                .map(|beta| beta.header_value())
                .collect::<Vec<_>>()
                .join(",")
                .into_bytes()
        })
    }

    pub(crate) fn has_beta(&self, beta: AnthropicBeta) -> bool {
        self.betas.binary_search(&beta).is_ok()
    }
}

impl fmt::Debug for AnthropicConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicConfig")
            .field("endpoint", &self.endpoint)
            .field("credential", &self.credential)
            .field("profile", &self.profile)
            .field("betas", &self.betas)
            .field("http_limits", &self.http_limits)
            .field("framing_limits", &self.framing_limits)
            .field("retry_policy", &self.retry_policy)
            .finish()
    }
}
