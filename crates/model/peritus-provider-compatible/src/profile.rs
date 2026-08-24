//! Separately validated Responses and Chat Completions compatibility contracts.

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityProvenance, CapabilityState, OutputLimitEnforcement,
    ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_provider_core::ProviderCoreError;

use crate::error;

/// Explicitly mapped provider-neutral request field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestField {
    /// Ordered role/content messages.
    Messages,
    /// Function declarations, calls, results, and tool selection.
    Tools,
    /// Parallel function-call switch.
    ParallelTools,
    /// Strict JSON Schema output.
    StrictStructuredOutput,
    /// Inline or HTTPS image input.
    ImageInput,
    /// Temperature sampling.
    Temperature,
    /// Nucleus sampling.
    TopP,
    /// Deterministic seed.
    Seed,
    /// Stop sequences.
    StopSequences,
    /// Final token-usage details.
    Usage,
}

impl RequestField {
    const fn bit(self) -> u32 {
        match self {
            Self::Messages => 1 << 0,
            Self::Tools => 1 << 1,
            Self::ParallelTools => 1 << 2,
            Self::StrictStructuredOutput => 1 << 3,
            Self::ImageInput => 1 << 4,
            Self::Temperature => 1 << 5,
            Self::TopP => 1 << 6,
            Self::Seed => 1 << 7,
            Self::StopSequences => 1 << 8,
            Self::Usage => 1 << 9,
        }
    }
}

/// Exact compatible streaming grammar selected by the profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventMapping {
    /// Typed Responses lifecycle and output events with sequence numbers.
    ResponsesV1,
    /// `chat.completion.chunk` choice/delta events followed by `[DONE]`.
    ChatCompletionsV1,
}

/// Stream framing required by both supported dialects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamFraming {
    /// UTF-8 server-sent events with bounded data frames.
    Sse,
}

/// Compatible response identity guarantee.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseIdSemantics {
    /// Every stream must expose one stable opaque identity before output.
    RequiredStable,
}

/// Protection available when recreating a compatible create request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateReplayGuarantee {
    /// No idempotency claim; only pre-submit connect failures and explicit rejections may retry.
    None,
}

/// Immutable wire contract paired with one protocol profile revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibleContract {
    mapping: EventMapping,
    framing: StreamFraming,
    response_ids: ResponseIdSemantics,
    create_replay: CreateReplayGuarantee,
    fields: u32,
}

impl CompatibleContract {
    const fn responses(profile: &ProviderProfile) -> Self {
        Self {
            mapping: EventMapping::ResponsesV1,
            framing: StreamFraming::Sse,
            response_ids: ResponseIdSemantics::RequiredStable,
            create_replay: CreateReplayGuarantee::None,
            fields: request_fields(profile, false),
        }
    }

    const fn chat(profile: &ProviderProfile) -> Self {
        Self {
            mapping: EventMapping::ChatCompletionsV1,
            framing: StreamFraming::Sse,
            response_ids: ResponseIdSemantics::RequiredStable,
            create_replay: CreateReplayGuarantee::None,
            fields: request_fields(profile, true),
        }
    }

    /// Returns the exact stream-event mapping.
    #[must_use]
    pub const fn event_mapping(self) -> EventMapping {
        self.mapping
    }

    /// Returns the required framing.
    #[must_use]
    pub const fn framing(self) -> StreamFraming {
        self.framing
    }

    /// Returns the response-identity guarantee.
    #[must_use]
    pub const fn response_ids(self) -> ResponseIdSemantics {
        self.response_ids
    }

    /// Returns the create-request replay guarantee.
    #[must_use]
    pub const fn create_replay(self) -> CreateReplayGuarantee {
        self.create_replay
    }

    /// Returns whether one provider-neutral field has an exact wire mapping.
    #[must_use]
    pub const fn supports(self, field: RequestField) -> bool {
        self.fields & field.bit() != 0
    }
}

/// One exact compatible provider profile and its reviewed dialect contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibleProfile {
    provider: ProviderProfile,
    contract: CompatibleContract,
}

impl CompatibleProfile {
    /// Binds a `CompatibleResponses` profile to the fixed Responses-v1 mappings.
    ///
    /// # Errors
    ///
    /// Rejects wrong dialects, discovery-only claims, unknown capabilities, unsupported features,
    /// or provider-side retention/resume/cancellation guarantees.
    pub fn responses(provider: ProviderProfile) -> Result<Self, ProviderCoreError> {
        validate(&provider, WireDialect::CompatibleResponses)?;
        let contract = CompatibleContract::responses(&provider);
        Ok(Self { provider, contract })
    }

    /// Binds a `CompatibleChatCompletions` profile to fixed Chat Completions-v1 mappings.
    ///
    /// # Errors
    ///
    /// Rejects wrong dialects, discovery-only claims, unknown capabilities, unsupported features,
    /// or provider-side retention/resume/cancellation guarantees.
    pub fn chat_completions(provider: ProviderProfile) -> Result<Self, ProviderCoreError> {
        validate(&provider, WireDialect::CompatibleChatCompletions)?;
        let contract = CompatibleContract::chat(&provider);
        Ok(Self { provider, contract })
    }

    /// Returns the immutable protocol profile.
    #[must_use]
    pub const fn provider_profile(&self) -> &ProviderProfile {
        &self.provider
    }

    /// Returns the exact request/event compatibility contract.
    #[must_use]
    pub const fn contract(&self) -> CompatibleContract {
        self.contract
    }

    /// Returns the explicit profile revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.provider.revision()
    }

    /// Returns caller-owned retention semantics.
    #[must_use]
    pub const fn retention(&self) -> StateMode {
        self.provider.state_mode()
    }

    /// Returns the declared response-resume behavior.
    #[must_use]
    pub const fn resume(&self) -> ResumeKind {
        self.provider.resume_kind()
    }

    /// Returns the declared cancellation behavior.
    #[must_use]
    pub const fn cancellation(&self) -> CancellationKind {
        self.provider.cancellation_kind()
    }
}

fn validate(profile: &ProviderProfile, dialect: WireDialect) -> Result<(), ProviderCoreError> {
    if profile.dialect() != dialect
        || profile.provenance() == CapabilityProvenance::Discovered
        || profile.output_limit_enforcement() != OutputLimitEnforcement::ProviderEnforced
        || !profile.capabilities().supports(Capability::Streaming)
        || profile.state_mode() != StateMode::StatelessReplay
        || profile.resume_kind() != ResumeKind::Unsupported
        || profile.cancellation_kind() != CancellationKind::BestEffortLocalAbort
    {
        return Err(error::configuration(
            "compatible profile dialect, provenance, streaming, or lifecycle is not exact",
        ));
    }
    for (capability, state) in profile.capabilities().iter() {
        if state == CapabilityState::Unknown
            || state == CapabilityState::Supported && !supported_capability(capability)
        {
            return Err(error::configuration(
                "compatible profile contains an unknown or unmapped capability",
            ));
        }
    }
    if profile.capabilities().supports(Capability::ParallelToolCalls)
        && !profile.capabilities().supports(Capability::ToolCalls)
    {
        return Err(error::configuration("parallel compatible tools require mapped tool calls"));
    }
    Ok(())
}

const fn supported_capability(capability: Capability) -> bool {
    matches!(
        capability,
        Capability::Streaming
            | Capability::ToolCalls
            | Capability::ParallelToolCalls
            | Capability::StrictStructuredOutput
            | Capability::ImageInput
            | Capability::UsageDetail
            | Capability::RateLimitDetail
            | Capability::SamplingControls
    )
}

const fn request_fields(profile: &ProviderProfile, chat: bool) -> u32 {
    let mut fields = RequestField::Messages.bit();
    let capabilities = profile.capabilities();
    if capabilities.supports(Capability::ToolCalls) {
        fields |= RequestField::Tools.bit();
    }
    if capabilities.supports(Capability::ParallelToolCalls) {
        fields |= RequestField::ParallelTools.bit();
    }
    if capabilities.supports(Capability::StrictStructuredOutput) {
        fields |= RequestField::StrictStructuredOutput.bit();
    }
    if capabilities.supports(Capability::ImageInput) {
        fields |= RequestField::ImageInput.bit();
    }
    if capabilities.supports(Capability::SamplingControls) {
        fields |= RequestField::Temperature.bit() | RequestField::TopP.bit();
        if chat {
            fields |= RequestField::Seed.bit() | RequestField::StopSequences.bit();
        }
    }
    if capabilities.supports(Capability::UsageDetail) {
        fields |= RequestField::Usage.bit();
    }
    fields
}
