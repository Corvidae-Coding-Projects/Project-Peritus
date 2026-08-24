//! Capability identities and closed support matrices.

use crate::{ProtocolError, ProtocolErrorKind};

/// One independently negotiated provider feature.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Capability {
    /// Incremental response streaming.
    Streaming = 0,
    /// Application function calls.
    ToolCalls = 1,
    /// Multiple simultaneous function calls.
    ParallelToolCalls = 2,
    /// Strict JSON-Schema-constrained output.
    StrictStructuredOutput = 3,
    /// Provider prompt/context caching.
    PromptCaching = 4,
    /// Image input.
    ImageInput = 5,
    /// Audio input.
    AudioInput = 6,
    /// Document input.
    DocumentInput = 7,
    /// Model reasoning controls.
    ReasoningControls = 8,
    /// Visible reasoning summaries.
    ReasoningSummaries = 9,
    /// Exact provider cursor resumption.
    ResumableResponse = 10,
    /// Provider-confirmed server cancellation.
    ConfirmedCancellation = 11,
    /// Detailed usage counters.
    UsageDetail = 12,
    /// Structured rate-limit observations.
    RateLimitDetail = 13,
    /// Provider-stored conversational state.
    StoredState = 14,
    /// Explicit profile-owned extensions.
    ProviderExtensions = 15,
    /// Model-specific sampling and deterministic-seed controls.
    SamplingControls = 16,
}

impl Capability {
    pub(super) const ALL: [Self; 17] = [
        Self::Streaming,
        Self::ToolCalls,
        Self::ParallelToolCalls,
        Self::StrictStructuredOutput,
        Self::PromptCaching,
        Self::ImageInput,
        Self::AudioInput,
        Self::DocumentInput,
        Self::ReasoningControls,
        Self::ReasoningSummaries,
        Self::ResumableResponse,
        Self::ConfirmedCancellation,
        Self::UsageDetail,
        Self::RateLimitDetail,
        Self::StoredState,
        Self::ProviderExtensions,
        Self::SamplingControls,
    ];

    pub(super) const fn bit(self) -> u64 {
        match self {
            Self::Streaming => 1 << 0,
            Self::ToolCalls => 1 << 1,
            Self::ParallelToolCalls => 1 << 2,
            Self::StrictStructuredOutput => 1 << 3,
            Self::PromptCaching => 1 << 4,
            Self::ImageInput => 1 << 5,
            Self::AudioInput => 1 << 6,
            Self::DocumentInput => 1 << 7,
            Self::ReasoningControls => 1 << 8,
            Self::ReasoningSummaries => 1 << 9,
            Self::ResumableResponse => 1 << 10,
            Self::ConfirmedCancellation => 1 << 11,
            Self::UsageDetail => 1 << 12,
            Self::RateLimitDetail => 1 << 13,
            Self::StoredState => 1 << 14,
            Self::ProviderExtensions => 1 << 15,
            Self::SamplingControls => 1 << 16,
        }
    }

    pub(super) const fn known_mask() -> u64 {
        (1_u64 << Self::ALL.len()) - 1
    }

    /// Stable field name used in diagnostics and canonical bytes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::ToolCalls => "tool_calls",
            Self::ParallelToolCalls => "parallel_tool_calls",
            Self::StrictStructuredOutput => "strict_structured_output",
            Self::PromptCaching => "prompt_caching",
            Self::ImageInput => "image_input",
            Self::AudioInput => "audio_input",
            Self::DocumentInput => "document_input",
            Self::ReasoningControls => "reasoning_controls",
            Self::ReasoningSummaries => "reasoning_summaries",
            Self::ResumableResponse => "resumable_response",
            Self::ConfirmedCancellation => "confirmed_cancellation",
            Self::UsageDetail => "usage_detail",
            Self::RateLimitDetail => "rate_limit_detail",
            Self::StoredState => "stored_state",
            Self::ProviderExtensions => "provider_extensions",
            Self::SamplingControls => "sampling_controls",
        }
    }
}

/// Three-valued support state. Unknown never satisfies a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityState {
    /// Authoritatively supported by the bound profile revision.
    Supported,
    /// Authoritatively unsupported.
    Unsupported,
    /// Not proven either way.
    Unknown,
}

/// Compact complete capability truth table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityMatrix {
    supported: u64,
    unknown: u64,
}

impl CapabilityMatrix {
    /// Builds a matrix. Every omitted capability is unsupported.
    ///
    /// # Errors
    ///
    /// Rejects a capability listed as both supported and unknown.
    pub fn new(supported: &[Capability], unknown: &[Capability]) -> Result<Self, ProtocolError> {
        let supported = mask(supported);
        let unknown = mask(unknown);
        if supported & unknown != 0 {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidProfile,
                "capabilities",
                "a capability cannot be both supported and unknown",
            ));
        }
        Ok(Self { supported, unknown })
    }

    /// Returns the explicit support state.
    #[must_use]
    pub const fn state(self, capability: Capability) -> CapabilityState {
        let bit = capability.bit();
        if self.supported & bit != 0 {
            CapabilityState::Supported
        } else if self.unknown & bit != 0 {
            CapabilityState::Unknown
        } else {
            CapabilityState::Unsupported
        }
    }

    /// Returns whether support is proven.
    #[must_use]
    pub const fn supports(self, capability: Capability) -> bool {
        matches!(self.state(capability), CapabilityState::Supported)
    }

    /// Iterates all capability names and states in stable order.
    #[must_use]
    pub fn iter(self) -> std::array::IntoIter<(Capability, CapabilityState), 17> {
        Capability::ALL.map(|capability| (capability, self.state(capability))).into_iter()
    }

    pub(super) const fn supported_mask(self) -> u64 {
        self.supported
    }
}

fn mask(capabilities: &[Capability]) -> u64 {
    capabilities.iter().fold(0, |value, capability| value | capability.bit())
}
