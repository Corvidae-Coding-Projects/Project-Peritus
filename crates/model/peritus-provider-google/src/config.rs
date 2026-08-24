//! Immutable endpoint, credential, profile, and resource configuration.

use core::fmt;

use peritus_model_protocol::ProviderProfile;
use peritus_provider_core::{
    CredentialReference, Endpoint, FramingLimits, HttpLimits, ProviderCoreError, RetryPolicy,
};

use crate::profile::validate_google_profile;

/// Complete immutable configuration for one stable-v1 Google adapter instance.
#[derive(Clone)]
pub struct GoogleConfig {
    endpoint: Endpoint,
    credential: CredentialReference,
    profile: ProviderProfile,
    http_limits: HttpLimits,
    framing_limits: FramingLimits,
    retry_policy: RetryPolicy,
}

impl GoogleConfig {
    /// Creates a profile-bound configuration for a clean Google API origin.
    ///
    /// # Errors
    ///
    /// Rejects profile drift or an endpoint containing a base path or query. Production requests
    /// always append an exact stable-v1 route and never inherit an SDK version default.
    pub fn new(
        endpoint: Endpoint,
        credential: CredentialReference,
        profile: ProviderProfile,
        http_limits: HttpLimits,
        framing_limits: FramingLimits,
        retry_policy: RetryPolicy,
    ) -> Result<Self, ProviderCoreError> {
        validate_google_profile(&profile)?;
        if !clean_origin(&endpoint) {
            return Err(ProviderCoreError::configuration(
                "google_config",
                "Google endpoint must be a clean HTTP(S) origin without a path or query",
            ));
        }
        Ok(Self { endpoint, credential, profile, http_limits, framing_limits, retry_policy })
    }

    /// Returns the exact immutable provider profile.
    #[must_use]
    pub const fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    /// Returns the configured API origin.
    #[must_use]
    pub const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
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
}

impl fmt::Debug for GoogleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleConfig")
            .field("endpoint", &self.endpoint)
            .field("credential", &self.credential)
            .field("profile", &self.profile)
            .field("http_limits", &self.http_limits)
            .field("framing_limits", &self.framing_limits)
            .field("retry_policy", &self.retry_policy)
            .finish()
    }
}

fn clean_origin(endpoint: &Endpoint) -> bool {
    let value = endpoint.as_str();
    let Some((_scheme, remainder)) = value.split_once("://") else {
        return false;
    };
    remainder.find('/').is_some_and(|index| &remainder[index..] == "/")
}
