//! Immutable endpoint, credential, profile, and resource configuration.

use core::fmt;

use peritus_model_protocol::{ProviderProfile, WireDialect};
use peritus_provider_core::{
    CredentialReference, Endpoint, FramingLimits, HttpLimits, ProviderCoreError, RetryPolicy,
    catalog::{CatalogDialect, HttpCatalogDiscovery},
};

use crate::profile::validate_google_profile;

/// Complete immutable configuration for one stable-v1 Google adapter instance.
#[derive(Clone)]
pub struct GoogleConfig {
    endpoint: GoogleEndpointContract,
    credential: CredentialReference,
    profile: ProviderProfile,
    http_limits: HttpLimits,
    framing_limits: FramingLimits,
    retry_policy: RetryPolicy,
}

impl GoogleConfig {
    pub(crate) fn with_selected_profile(
        mut self,
        profile: ProviderProfile,
    ) -> Result<Self, ProviderCoreError> {
        validate_google_profile(&profile)?;
        self.endpoint.validate_dialect(profile.dialect())?;
        self.profile = profile;
        Ok(self)
    }

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
        let endpoint = GoogleEndpointContract::first_party(endpoint, profile.dialect())?;
        Ok(Self { endpoint, credential, profile, http_limits, framing_limits, retry_policy })
    }

    /// Creates the explicitly reviewed `OpenCode` Google gateway route.
    ///
    /// # Errors
    /// Rejects any endpoint outside `OpenCode`'s exact documented prefixes and validates the native
    /// protocol profile. Ordinary Google configuration still requires a clean origin.
    pub fn opencode_gateway(
        endpoint: Endpoint,
        credential: CredentialReference,
        profile: ProviderProfile,
        http_limits: HttpLimits,
        framing_limits: FramingLimits,
        retry_policy: RetryPolicy,
    ) -> Result<Self, ProviderCoreError> {
        validate_google_profile(&profile)?;
        let endpoint = GoogleEndpointContract::opencode(endpoint, profile.dialect())?;
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
        &self.endpoint.prefix
    }

    pub(crate) fn operation_endpoint(
        &self,
        dialect: WireDialect,
        model: &str,
    ) -> Result<Endpoint, ProviderCoreError> {
        self.endpoint.operation(dialect, model)
    }

    pub(crate) fn catalog_discovery(&self) -> HttpCatalogDiscovery {
        HttpCatalogDiscovery::new(
            self.endpoint.catalog.clone(),
            self.endpoint.catalog_dialect,
        )
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
            .field("endpoint", &self.endpoint.prefix)
            .field("endpoint_family", &self.endpoint.family)
            .field("api_version", &self.endpoint.version)
            .field("credential", &self.credential)
            .field("profile", &self.profile)
            .field("http_limits", &self.http_limits)
            .field("framing_limits", &self.framing_limits)
            .field("retry_policy", &self.retry_policy)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GoogleEndpointFamily {
    FirstParty,
    OpenCode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GoogleApiVersion {
    StableV1,
}

impl GoogleApiVersion {
    const fn path(self) -> &'static str {
        match self {
            Self::StableV1 => "v1",
        }
    }
}

#[derive(Clone)]
struct GoogleEndpointContract {
    prefix: Endpoint,
    family: GoogleEndpointFamily,
    version: GoogleApiVersion,
    dialect: WireDialect,
    catalog: Endpoint,
    catalog_dialect: CatalogDialect,
}

impl GoogleEndpointContract {
    fn first_party(
        prefix: Endpoint,
        dialect: WireDialect,
    ) -> Result<Self, ProviderCoreError> {
        if !clean_origin(&prefix) {
            return Err(ProviderCoreError::configuration(
                "google_config",
                "Google endpoint must be a clean HTTP(S) origin without a path or query",
            ));
        }
        Self::build(prefix, GoogleEndpointFamily::FirstParty, dialect)
    }

    fn opencode(prefix: Endpoint, dialect: WireDialect) -> Result<Self, ProviderCoreError> {
        if !matches!(prefix.as_str(), "https://opencode.ai/zen/" | "https://opencode.ai/zen/go/")
        {
            return Err(ProviderCoreError::configuration(
                "google_config",
                "unrecognized OpenCode Google gateway prefix",
            ));
        }
        Self::build(prefix, GoogleEndpointFamily::OpenCode, dialect)
    }

    fn build(
        prefix: Endpoint,
        family: GoogleEndpointFamily,
        dialect: WireDialect,
    ) -> Result<Self, ProviderCoreError> {
        let supported = match family {
            GoogleEndpointFamily::FirstParty => matches!(
                dialect,
                WireDialect::GeminiInteractionsV1 | WireDialect::GeminiGenerateContentV1
            ),
            GoogleEndpointFamily::OpenCode => {
                dialect == WireDialect::GeminiGenerateContentV1
            }
        };
        if !supported {
            return Err(ProviderCoreError::configuration(
                "google_config",
                "Google endpoint family does not support the selected wire dialect",
            ));
        }
        let version = GoogleApiVersion::StableV1;
        let (catalog_route, catalog_dialect) = match family {
            GoogleEndpointFamily::FirstParty => (
                format!("{}/models?pageSize=1000", version.path()),
                CatalogDialect::GoogleV1,
            ),
            GoogleEndpointFamily::OpenCode => {
                (format!("{}/models", version.path()), CatalogDialect::OpenAi)
            }
        };
        let catalog = prefix.append_route(&catalog_route)?;
        Ok(Self { prefix, family, version, dialect, catalog, catalog_dialect })
    }

    fn validate_dialect(&self, dialect: WireDialect) -> Result<(), ProviderCoreError> {
        if dialect != self.dialect {
            return Err(ProviderCoreError::configuration(
                "google_config",
                "selected Google profile changed the endpoint's wire dialect",
            ));
        }
        Ok(())
    }

    fn operation(
        &self,
        dialect: WireDialect,
        model: &str,
    ) -> Result<Endpoint, ProviderCoreError> {
        self.validate_dialect(dialect)?;
        let route = match dialect {
            WireDialect::GeminiInteractionsV1
                if self.family == GoogleEndpointFamily::FirstParty =>
            {
                format!("{}/interactions", self.version.path())
            }
            WireDialect::GeminiGenerateContentV1 => format!(
                "{}/models/{model}:streamGenerateContent?alt=sse",
                self.version.path()
            ),
            _ => {
                return Err(ProviderCoreError::configuration(
                    "google_config",
                    "Google endpoint contract has no reviewed route for this dialect",
                ));
            }
        };
        self.prefix.append_route(&route)
    }
}

fn clean_origin(endpoint: &Endpoint) -> bool {
    let value = endpoint.as_str();
    let Some((_scheme, remainder)) = value.split_once("://") else {
        return false;
    };
    remainder.find('/').is_some_and(|index| &remainder[index..] == "/")
}
