//! The complete immutable model request and its canonical identity.

use peritus_types::ProviderProfileId;

use super::{RequestOptions, validation};
use crate::{
    Message, NegotiatedCapabilities, ParallelToolPolicy, ProtocolError, ProtocolLimits,
    ProtocolVersion, ProviderName, ProviderProfile, RequestId, ToolChoice, ToolDefinition,
    WireDialect,
};

/// Complete provider-neutral model request bound to one immutable profile revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRequest {
    protocol: ProtocolVersion,
    profile_id: ProviderProfileId,
    profile_revision: u64,
    provider: ProviderName,
    dialect: WireDialect,
    request_id: RequestId,
    model: crate::ModelName,
    negotiated: NegotiatedCapabilities,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
    tool_choice: ToolChoice,
    parallel_tools: ParallelToolPolicy,
    options: RequestOptions,
}

impl ModelRequest {
    /// Creates and completely validates one revision-bound request.
    ///
    /// # Errors
    ///
    /// Rejects profile drift, unsupported behavior, duplicates, and exceeded bounds.
    #[allow(clippy::too_many_arguments, reason = "constructor binds the complete request boundary")]
    pub fn new(
        profile: &ProviderProfile,
        negotiated: NegotiatedCapabilities,
        request_id: RequestId,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
        tool_choice: ToolChoice,
        parallel_tools: ParallelToolPolicy,
        options: RequestOptions,
        limits: ProtocolLimits,
    ) -> Result<Self, ProtocolError> {
        let identity_matches = profile.profile_id() == negotiated.profile_id();
        let revision_matches = profile.revision() == negotiated.profile_revision();
        if !identity_matches || !revision_matches {
            return Err(validation::invalid(
                "profile",
                "negotiated capabilities belong to another profile",
            ));
        }
        validation::request(negotiated, &messages, &tools, parallel_tools, &options, limits)?;
        Ok(Self {
            protocol: ProtocolVersion::V1,
            profile_id: profile.profile_id(),
            profile_revision: profile.revision(),
            provider: profile.provider().clone(),
            dialect: profile.dialect(),
            request_id,
            model: profile.model().clone(),
            negotiated,
            messages,
            tools,
            tool_choice,
            parallel_tools,
            options,
        })
    }

    /// Protocol version.
    #[must_use]
    pub const fn protocol(&self) -> ProtocolVersion {
        self.protocol
    }
    /// Bound profile identity.
    #[must_use]
    pub const fn profile_id(&self) -> ProviderProfileId {
        self.profile_id
    }
    /// Bound profile revision.
    #[must_use]
    pub const fn profile_revision(&self) -> u64 {
        self.profile_revision
    }
    /// Provider family.
    #[must_use]
    pub const fn provider(&self) -> &ProviderName {
        &self.provider
    }
    /// Selected wire dialect.
    #[must_use]
    pub const fn dialect(&self) -> WireDialect {
        self.dialect
    }
    /// Caller request identity.
    #[must_use]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }
    /// Exact model name.
    #[must_use]
    pub const fn model(&self) -> &crate::ModelName {
        &self.model
    }
    /// Negotiated features and limits.
    #[must_use]
    pub const fn negotiated(&self) -> NegotiatedCapabilities {
        self.negotiated
    }
    /// Ordered messages.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }
    /// Function declarations.
    #[must_use]
    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }
    /// Tool-selection policy.
    #[must_use]
    pub const fn tool_choice(&self) -> &ToolChoice {
        &self.tool_choice
    }
    /// Parallel-call policy.
    #[must_use]
    pub const fn parallel_tool_policy(&self) -> ParallelToolPolicy {
        self.parallel_tools
    }
    /// Other request policies.
    #[must_use]
    pub const fn options(&self) -> &RequestOptions {
        &self.options
    }

    /// Encodes exact version-one semantic request bytes for replay and idempotency.
    ///
    /// The caller request ID and credentials are deliberately excluded.
    ///
    /// # Errors
    ///
    /// Returns a protocol limit error if an internal canonical bound is exceeded.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        crate::canonical::request_bytes(self)
    }

    /// Computes the exact canonical request fingerprint.
    ///
    /// # Errors
    ///
    /// Returns a protocol limit error if canonical encoding fails.
    pub fn fingerprint(&self) -> Result<crate::RequestFingerprint, ProtocolError> {
        self.canonical_bytes()
            .map(|bytes| crate::RequestFingerprint::new(peritus_codec::sha256(&bytes)))
    }

    /// Derives a stable printable idempotency key from exact semantic request bytes.
    ///
    /// Adapters send this value only when their documented profile supports create idempotency.
    ///
    /// # Errors
    ///
    /// Returns a protocol limit error if canonical encoding fails.
    pub fn idempotency_key(&self) -> Result<crate::IdempotencyKey, ProtocolError> {
        let digest = self.fingerprint()?.digest();
        let mut value = String::with_capacity(75);
        value.push_str("peritus-v1-");
        for byte in digest.as_bytes() {
            use core::fmt::Write as _;
            let _ = write!(value, "{byte:02x}");
        }
        crate::IdempotencyKey::new(value)
    }
}
