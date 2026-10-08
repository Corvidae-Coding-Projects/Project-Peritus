//! The complete immutable model request and its canonical identity.

use peritus_types::{ArtifactId, ProviderProfileId, Sha256Digest};

use super::{Continuation, RequestOptions, validation};
use crate::{
    Message, NegotiatedCapabilities, ParallelToolPolicy, ProtocolError, ProtocolLimits,
    ProtocolVersion, ProviderName, ProviderProfile, RequestId, ResumeKind, ToolChoice,
    ToolDefinition, WireDialect,
};

/// Complete provider-neutral model request bound to one immutable profile revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRequest {
    protocol: ProtocolVersion,
    profile_id: ProviderProfileId,
    profile_revision: u64,
    provider: ProviderName,
    dialect: WireDialect,
    resume_kind: ResumeKind,
    request_id: RequestId,
    model: crate::ModelName,
    negotiated: NegotiatedCapabilities,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
    tool_choice: ToolChoice,
    parallel_tools: ParallelToolPolicy,
    options: RequestOptions,
    limits: ProtocolLimits,
    fingerprint: crate::RequestFingerprint,
    local_session_directory: Option<std::path::PathBuf>,
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
        validation::request(
            negotiated,
            &messages,
            &tools,
            &tool_choice,
            parallel_tools,
            &options,
            profile.resume_kind(),
            limits,
        )?;
        let mut request = Self {
            protocol: ProtocolVersion::V1,
            profile_id: profile.profile_id(),
            profile_revision: profile.revision(),
            provider: profile.provider().clone(),
            dialect: profile.dialect(),
            resume_kind: profile.resume_kind(),
            request_id,
            model: profile.model().clone(),
            negotiated,
            messages,
            tools,
            tool_choice,
            parallel_tools,
            options,
            limits,
            fingerprint: crate::RequestFingerprint::new(Sha256Digest::new([0; 32])),
            local_session_directory: None,
        };
        request.fingerprint =
            crate::RequestFingerprint::new(crate::canonical::request_digest(&request)?);
        Ok(request)
    }

    /// Binds native runtime storage to a host-owned task and role directory.
    ///
    /// This local transport metadata grants no provider continuation or idempotency guarantee.
    /// The complete canonical request remains the governing conversation projection.
    #[must_use]
    pub fn with_local_session_directory(mut self, directory: std::path::PathBuf) -> Self {
        self.local_session_directory = Some(directory);
        self
    }

    /// Binds an observed provider continuation to a retry of this exact logical request.
    ///
    /// The immutable profile, negotiated capabilities, messages, tools, native session directory,
    /// and caller request identity are retained. Complete request validation is repeated after the
    /// continuation is installed.
    ///
    /// # Errors
    ///
    /// Returns a protocol error when the continuation requires an unnegotiated capability.
    pub fn with_continuation(
        mut self,
        continuation: Continuation,
        limits: ProtocolLimits,
    ) -> Result<Self, ProtocolError> {
        if limits != self.limits {
            return Err(validation::invalid(
                "protocol_limits",
                "continuation must retain the request's exact admitted protocol limits",
            ));
        }
        self.options = self.options.with_continuation(continuation);
        self.validate_admission()?;
        self.fingerprint =
            crate::RequestFingerprint::new(crate::canonical::request_digest(&self)?);
        Ok(self)
    }

    /// Resolves authenticated artifact media for one immediate provider projection.
    ///
    /// The semantic request, fingerprint, and durable archive retain artifact identities. Only the
    /// returned in-memory copy exposes bytes to the selected provider adapter.
    ///
    /// # Errors
    /// Returns the resolver's typed error or a protocol error converted into it when bytes do not
    /// match their exact digest or the selected provider's per-image limit.
    pub fn resolve_artifacts<E>(
        mut self,
        mut resolver: impl FnMut(ArtifactId, Sha256Digest, u64) -> Result<Vec<u8>, E>,
    ) -> Result<Self, E>
    where
        E: From<ProtocolError>,
    {
        let maximum = self.negotiated.limits().max_inline_media_bytes();
        for message in &mut self.messages {
            for block in message.content_mut() {
                let media = match block {
                    crate::ContentBlock::Image(media)
                    | crate::ContentBlock::Audio(media)
                    | crate::ContentBlock::Document(media) => media,
                    crate::ContentBlock::Text(_)
                    | crate::ContentBlock::ToolCall(_)
                    | crate::ContentBlock::ToolResult(_)
                    | crate::ContentBlock::Refusal(_)
                    | crate::ContentBlock::Reasoning(_)
                    | crate::ContentBlock::ProviderExtension(_) => continue,
                };
                let Some((artifact, digest)) = media.artifact_reference() else {
                    continue;
                };
                let bytes = resolver(artifact, digest, maximum)?;
                *media = media
                    .clone()
                    .with_resolved_artifact(bytes, maximum)
                    .map_err(E::from)?;
            }
        }
        Ok(self)
    }

    /// Returns the host-owned native runtime storage namespace, when supplied.
    #[must_use]
    pub fn local_session_directory(&self) -> Option<&std::path::Path> {
        self.local_session_directory.as_deref()
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

    /// Exact protocol limits that admitted this request.
    #[must_use]
    pub const fn protocol_limits(&self) -> ProtocolLimits {
        self.limits
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

    /// Encodes the same canonical bytes while enforcing a narrower host storage ceiling.
    ///
    /// The writer rejects before appending a field that would exceed the ceiling. This does
    /// not encode the whole request and truncate it, and does not change its fingerprint.
    ///
    /// # Errors
    /// Rejects a zero ceiling or a request that cannot fit completely.
    pub fn canonical_bytes_bounded(&self, maximum_bytes: usize) -> Result<Vec<u8>, ProtocolError> {
        crate::canonical::request_bytes_bounded(self, maximum_bytes)
    }

    /// Computes the exact canonical request fingerprint.
    ///
    /// # Errors
    ///
    /// Returns a protocol limit error if canonical encoding fails.
    pub fn fingerprint(&self) -> Result<crate::RequestFingerprint, ProtocolError> {
        Ok(self.fingerprint)
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

    pub(crate) fn validate_admission(&self) -> Result<(), ProtocolError> {
        validation::request(
            self.negotiated,
            &self.messages,
            &self.tools,
            &self.tool_choice,
            self.parallel_tools,
            &self.options,
            self.resume_kind,
            self.limits,
        )
    }
}
