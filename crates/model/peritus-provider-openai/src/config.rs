//! Validated production endpoint, credential, and routing configuration.

use core::fmt;
use std::time::Duration;

use peritus_model_protocol::ProtocolLimits;
use peritus_provider_core::{
    CredentialReference, Endpoint, FramingLimits, HttpLimits, ProviderCoreError, RetryPolicy,
};

const OPENAI_ENDPOINT: &str = "https://api.openai.com";
const MAX_ROUTING_ID_BYTES: usize = 512;

#[derive(Clone, Eq, PartialEq)]
struct RoutingId(String);

impl RoutingId {
    fn organization(value: String) -> Result<Self, ProviderCoreError> {
        Self::new(value, "org-")
    }

    fn project(value: String) -> Result<Self, ProviderCoreError> {
        Self::new(value, "proj_")
    }

    fn new(value: String, prefix: &str) -> Result<Self, ProviderCoreError> {
        if value.len() <= prefix.len()
            || value.len() > MAX_ROUTING_ID_BYTES
            || !value.starts_with(prefix)
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ProviderCoreError::configuration(
                "openai_config",
                "OpenAI routing identity is malformed or exceeds its byte bound",
            ));
        }
        Ok(Self(value))
    }

    const fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Debug for RoutingId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoutingId([redacted])")
    }
}

/// Production `OpenAI` endpoint, credential reference, routing, and resource limits.
#[derive(Clone)]
pub struct OpenAiConfig {
    endpoint: Endpoint,
    credential: CredentialReference,
    organization: Option<RoutingId>,
    project: Option<RoutingId>,
    http_limits: HttpLimits,
    framing_limits: FramingLimits,
    protocol_limits: ProtocolLimits,
    retry_policy: RetryPolicy,
}

impl OpenAiConfig {
    /// Creates production configuration for `https://api.openai.com/v1/responses`.
    ///
    /// Credentials remain unresolved until immediately before request encoding and submission.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe configuration error if the fixed endpoint cannot be represented.
    pub fn new(credential: CredentialReference) -> Result<Self, ProviderCoreError> {
        Self::from_endpoint(Endpoint::new(OPENAI_ENDPOINT.to_owned())?, credential)
    }

    /// Selects an explicit `OpenAI` organization routing identity.
    ///
    /// # Errors
    ///
    /// Rejects values outside `OpenAI`'s bounded `org-` identity form.
    pub fn with_organization(mut self, organization: String) -> Result<Self, ProviderCoreError> {
        self.organization = Some(RoutingId::organization(organization)?);
        Ok(self)
    }

    /// Selects an explicit `OpenAI` project routing identity.
    ///
    /// # Errors
    ///
    /// Rejects values outside `OpenAI`'s bounded `proj_` identity form.
    pub fn with_project(mut self, project: String) -> Result<Self, ProviderCoreError> {
        self.project = Some(RoutingId::project(project)?);
        Ok(self)
    }

    /// Replaces the bounded retry policy used for connection failures and explicit temporary
    /// provider rejections.
    #[must_use]
    pub const fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    fn from_endpoint(
        endpoint: Endpoint,
        credential: CredentialReference,
    ) -> Result<Self, ProviderCoreError> {
        Ok(Self {
            endpoint,
            credential,
            organization: None,
            project: None,
            http_limits: HttpLimits::PRODUCTION,
            framing_limits: FramingLimits::PRODUCTION,
            protocol_limits: ProtocolLimits::PRODUCTION,
            retry_policy: RetryPolicy::new(
                3,
                [
                    Duration::from_millis(100),
                    Duration::from_secs(2),
                    Duration::from_secs(2),
                    Duration::from_secs(10),
                ],
                64 * 1024 * 1024,
            )?,
        })
    }

    pub(crate) const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub(crate) const fn credential(&self) -> &CredentialReference {
        &self.credential
    }

    pub(crate) fn organization(&self) -> Option<&[u8]> {
        self.organization.as_ref().map(RoutingId::as_bytes)
    }

    pub(crate) fn project(&self) -> Option<&[u8]> {
        self.project.as_ref().map(RoutingId::as_bytes)
    }

    pub(crate) const fn http_limits(&self) -> HttpLimits {
        self.http_limits
    }

    pub(crate) const fn framing_limits(&self) -> FramingLimits {
        self.framing_limits
    }

    pub(crate) const fn protocol_limits(&self) -> ProtocolLimits {
        self.protocol_limits
    }

    pub(crate) const fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        endpoint: Endpoint,
        credential: CredentialReference,
    ) -> Result<Self, ProviderCoreError> {
        Self::from_endpoint(endpoint, credential)
    }
}

impl fmt::Debug for OpenAiConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiConfig")
            .field("endpoint", &self.endpoint)
            .field("credential", &self.credential)
            .field("organization", &self.organization)
            .field("project", &self.project)
            .field("http_limits", &self.http_limits)
            .field("framing_limits", &self.framing_limits)
            .field("protocol_limits", &self.protocol_limits)
            .field("retry_policy", &self.retry_policy)
            .finish()
    }
}
