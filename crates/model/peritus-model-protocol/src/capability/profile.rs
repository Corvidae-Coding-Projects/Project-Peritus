//! Immutable provider profiles, model limits, and lifecycle semantics.

use peritus_types::ProviderProfileId;

use super::{Capability, CapabilityMatrix};
use crate::{ModelName, ProtocolError, ProtocolErrorKind, ProtocolVersion, ProviderName};

/// Origin of one profile's capability claims.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityProvenance {
    /// Maintained from official provider documentation.
    Profiled,
    /// Returned by a provider discovery endpoint.
    Discovered,
    /// Proven by a profile-bound disposable conformance probe.
    Probed,
}

/// Model-scoped numeric limits discovered or profiled independently of protocol ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "max_ distinguishes profile ceilings from later observations"
)]
pub struct ModelLimits {
    max_input_tokens: u64,
    max_output_tokens: u64,
    max_tools: u32,
    max_parallel_tool_calls: u32,
    max_inline_media_bytes: u64,
}

impl ModelLimits {
    /// Creates nonzero model limits.
    ///
    /// # Errors
    ///
    /// Rejects zero limits or parallel calls wider than the tool count.
    pub fn new(
        max_input_tokens: u64,
        max_output_tokens: u64,
        max_tools: u32,
        max_parallel_tool_calls: u32,
        max_inline_media_bytes: u64,
    ) -> Result<Self, ProtocolError> {
        if max_input_tokens == 0
            || max_output_tokens == 0
            || max_tools == 0
            || max_parallel_tool_calls == 0
            || max_parallel_tool_calls > max_tools
            || max_inline_media_bytes == 0
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidLimit,
                "model_limits",
                "model limits must be nonzero and parallel calls cannot exceed tools",
            ));
        }
        Ok(Self {
            max_input_tokens,
            max_output_tokens,
            max_tools,
            max_parallel_tool_calls,
            max_inline_media_bytes,
        })
    }

    /// Maximum input tokens accepted by the model.
    #[must_use]
    pub const fn max_input_tokens(self) -> u64 {
        self.max_input_tokens
    }
    /// Maximum output tokens accepted by the model.
    #[must_use]
    pub const fn max_output_tokens(self) -> u64 {
        self.max_output_tokens
    }
    /// Maximum declared tools.
    #[must_use]
    pub const fn max_tools(self) -> u32 {
        self.max_tools
    }
    /// Maximum simultaneous calls.
    #[must_use]
    pub const fn max_parallel_tool_calls(self) -> u32 {
        self.max_parallel_tool_calls
    }
    /// Maximum inline media bytes.
    #[must_use]
    pub const fn max_inline_media_bytes(self) -> u64 {
        self.max_inline_media_bytes
    }

    pub(super) const fn intersect(self, requested: Self) -> Self {
        Self {
            max_input_tokens: min_u64(self.max_input_tokens, requested.max_input_tokens),
            max_output_tokens: min_u64(self.max_output_tokens, requested.max_output_tokens),
            max_tools: min_u32(self.max_tools, requested.max_tools),
            max_parallel_tool_calls: min_u32(
                self.max_parallel_tool_calls,
                requested.max_parallel_tool_calls,
            ),
            max_inline_media_bytes: min_u64(
                self.max_inline_media_bytes,
                requested.max_inline_media_bytes,
            ),
        }
    }

    pub(super) const fn is_within(self, ceiling: Self) -> bool {
        self.max_input_tokens <= ceiling.max_input_tokens
            && self.max_output_tokens <= ceiling.max_output_tokens
            && self.max_tools <= ceiling.max_tools
            && self.max_parallel_tool_calls <= ceiling.max_parallel_tool_calls
            && self.max_inline_media_bytes <= ceiling.max_inline_media_bytes
    }
}

/// Provider wire family selected by an immutable profile revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireDialect {
    /// First-party `OpenAI` Responses.
    OpenAiResponses,
    /// First-party Anthropic Messages.
    AnthropicMessages,
    /// First-party stable-v1 Gemini Interactions.
    GeminiInteractionsV1,
    /// First-party stable-v1 Gemini Generate Content.
    GeminiGenerateContentV1,
    /// Explicit compatible Responses dialect.
    CompatibleResponses,
    /// Explicit compatible Chat Completions dialect.
    CompatibleChatCompletions,
    /// Account-backed `OpenAI` Codex executable runtime.
    OpenAiCodexRuntime,
    /// Account-backed Anthropic Claude executable runtime.
    AnthropicClaudeRuntime,
}

/// Strength of the provider's output-token ceiling for one request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputLimitEnforcement {
    /// The provider receives and enforces the requested output-token ceiling.
    ProviderEnforced,
    /// The ceiling is advisory because the owned runtime exposes no exact control.
    Advisory,
}

/// Provider-side conversational state semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateMode {
    /// Caller replays all necessary state.
    StatelessReplay,
    /// Provider stores state referenced by an opaque identity.
    ProviderStored,
    /// Provider stores background work and supports cursor retrieval.
    BackgroundResumable,
}

/// Documented response continuation strength.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeKind {
    /// No exact stream resumption.
    Unsupported,
    /// A new request semantically continues prior output.
    SemanticContinuation,
    /// An exact provider event cursor resumes the same stored response.
    ExactCursor,
}

/// Documented cancellation strength.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationKind {
    /// No cancellation behavior is exposed.
    Unsupported,
    /// Local transport work stops without provider acknowledgement.
    BestEffortLocalAbort,
    /// Provider acknowledges cancellation of stored/background work.
    Confirmed,
}

/// Immutable provider/model compatibility profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderProfile {
    profile_id: ProviderProfileId,
    revision: u64,
    provider: ProviderName,
    model: ModelName,
    protocol: ProtocolVersion,
    dialect: WireDialect,
    capabilities: CapabilityMatrix,
    provenance: CapabilityProvenance,
    limits: ModelLimits,
    output_limit: OutputLimitEnforcement,
    state_mode: StateMode,
    resume: ResumeKind,
    cancellation: CancellationKind,
}

impl ProviderProfile {
    /// Creates a revision-bound profile and checks cross-field guarantees.
    ///
    /// # Errors
    ///
    /// Rejects revision zero or a lifecycle guarantee not backed by its capability flag.
    #[allow(
        clippy::too_many_arguments,
        reason = "constructor binds the complete immutable profile"
    )]
    pub fn new(
        profile_id: ProviderProfileId,
        revision: u64,
        provider: ProviderName,
        model: ModelName,
        dialect: WireDialect,
        capabilities: CapabilityMatrix,
        provenance: CapabilityProvenance,
        limits: ModelLimits,
        output_limit: OutputLimitEnforcement,
        state_mode: StateMode,
        resume: ResumeKind,
        cancellation: CancellationKind,
    ) -> Result<Self, ProtocolError> {
        if revision == 0
            || matches!(resume, ResumeKind::ExactCursor)
                && !capabilities.supports(Capability::ResumableResponse)
            || matches!(cancellation, CancellationKind::Confirmed)
                && !capabilities.supports(Capability::ConfirmedCancellation)
            || !matches!(state_mode, StateMode::StatelessReplay)
                && !capabilities.supports(Capability::StoredState)
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidProfile,
                "provider_profile",
                "profile revision or lifecycle guarantees contradict capabilities",
            ));
        }
        Ok(Self {
            profile_id,
            revision,
            provider,
            model,
            protocol: ProtocolVersion::V1,
            dialect,
            capabilities,
            provenance,
            limits,
            output_limit,
            state_mode,
            resume,
            cancellation,
        })
    }

    /// Stable profile identity.
    #[must_use]
    pub const fn profile_id(&self) -> ProviderProfileId {
        self.profile_id
    }
    /// Nonzero immutable profile revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Provider family name.
    #[must_use]
    pub const fn provider(&self) -> &ProviderName {
        &self.provider
    }
    /// Exact model name.
    #[must_use]
    pub const fn model(&self) -> &ModelName {
        &self.model
    }
    /// Model protocol version.
    #[must_use]
    pub const fn protocol(&self) -> ProtocolVersion {
        self.protocol
    }
    /// Selected wire dialect.
    #[must_use]
    pub const fn dialect(&self) -> WireDialect {
        self.dialect
    }
    /// Capability matrix.
    #[must_use]
    pub const fn capabilities(&self) -> CapabilityMatrix {
        self.capabilities
    }
    /// Claim provenance.
    #[must_use]
    pub const fn provenance(&self) -> CapabilityProvenance {
        self.provenance
    }
    /// Model limits.
    #[must_use]
    pub const fn limits(&self) -> ModelLimits {
        self.limits
    }
    /// Output-token ceiling enforcement strength.
    #[must_use]
    pub const fn output_limit_enforcement(&self) -> OutputLimitEnforcement {
        self.output_limit
    }
    /// State behavior.
    #[must_use]
    pub const fn state_mode(&self) -> StateMode {
        self.state_mode
    }
    /// Continuation behavior.
    #[must_use]
    pub const fn resume_kind(&self) -> ResumeKind {
        self.resume
    }
    /// Cancellation behavior.
    #[must_use]
    pub const fn cancellation_kind(&self) -> CancellationKind {
        self.cancellation
    }
}

const fn min_u64(left: u64, right: u64) -> u64 {
    if left < right { left } else { right }
}

const fn min_u32(left: u32, right: u32) -> u32 {
    if left < right { left } else { right }
}
