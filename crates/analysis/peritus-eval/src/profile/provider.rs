//! Canonical E3 snapshots of public C5 provider and model-control contracts.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityProvenance, CapabilityState, OutputLimitEnforcement,
    ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_types::{ProviderProfileId, Sha256Digest};

use crate::{
    EvaluationError, EvaluationErrorKind, EvaluationOperation, EvaluationRecovery,
    SeedDeliveryPolicy,
};

const PROVIDER_DOMAIN: &[u8] = b"peritus.evaluation.provider-snapshot.v1\0";
const MODEL_DOMAIN: &[u8] = b"peritus.evaluation.model-controls.v1\0";

/// Exact canonical snapshot of every public immutable C5 profile field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrozenProviderSnapshot {
    profile_id: ProviderProfileId,
    revision: u64,
    digest: Sha256Digest,
    sampling_controls: bool,
}

impl FrozenProviderSnapshot {
    /// Captures the complete public C5 profile without trusting a caller digest.
    ///
    /// # Errors
    /// Returns a codec-bound error if public profile strings exceed production limits.
    pub fn capture(profile: &ProviderProfile) -> Result<Self, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(PROVIDER_DOMAIN).map_err(codec)?;
        writer.write_fixed(profile.profile_id().as_bytes()).map_err(codec)?;
        writer.write_u64(profile.revision()).map_err(codec)?;
        writer.write_str(profile.provider().as_str()).map_err(codec)?;
        writer.write_str(profile.model().as_str()).map_err(codec)?;
        writer.write_u16(profile.protocol().major()).map_err(codec)?;
        writer.write_u16(profile.protocol().minor()).map_err(codec)?;
        writer.write_u8(dialect_tag(profile.dialect())).map_err(codec)?;
        for (capability, state) in profile.capabilities().iter() {
            writer.write_str(capability.name()).map_err(codec)?;
            writer.write_u8(capability_state_tag(state)).map_err(codec)?;
        }
        writer.write_u8(provenance_tag(profile.provenance())).map_err(codec)?;
        let limits = profile.limits();
        writer.write_u64(limits.max_input_tokens()).map_err(codec)?;
        writer.write_u64(limits.max_output_tokens()).map_err(codec)?;
        writer.write_u32(limits.max_tools()).map_err(codec)?;
        writer.write_u32(limits.max_parallel_tool_calls()).map_err(codec)?;
        writer.write_u64(limits.max_inline_media_bytes()).map_err(codec)?;
        writer.write_u8(output_tag(profile.output_limit_enforcement())).map_err(codec)?;
        writer.write_u8(state_tag(profile.state_mode())).map_err(codec)?;
        writer.write_u8(resume_tag(profile.resume_kind())).map_err(codec)?;
        writer.write_u8(cancellation_tag(profile.cancellation_kind())).map_err(codec)?;
        Ok(Self {
            profile_id: profile.profile_id(),
            revision: profile.revision(),
            digest: peritus_codec::sha256(&writer.into_bytes()),
            sampling_controls: profile.capabilities().supports(Capability::SamplingControls),
        })
    }
    /// Profile identity.
    #[must_use]
    pub const fn profile_id(self) -> ProviderProfileId {
        self.profile_id
    }
    /// Profile revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Complete snapshot digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Whether the profile supports provider-delivered seeds.
    #[must_use]
    pub const fn supports_sampling_controls(self) -> bool {
        self.sampling_controls
    }
}

/// Frozen E3-owned model controls used to derive per-rollout C5 requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrozenModelControls {
    maximum_output_tokens: u64,
    temperature_millionths: Option<u32>,
    top_p_millionths: Option<u32>,
    request_template_digest: Sha256Digest,
    seed_delivery: SeedDeliveryPolicy,
    digest: Sha256Digest,
}

impl FrozenModelControls {
    /// Creates complete checked model controls.
    ///
    /// # Errors
    /// Rejects invalid numeric controls or required seed delivery unsupported by the C5 snapshot.
    pub fn new(
        maximum_output_tokens: u64,
        temperature_millionths: Option<u32>,
        top_p_millionths: Option<u32>,
        request_template_digest: Sha256Digest,
        seed_delivery: SeedDeliveryPolicy,
        provider: FrozenProviderSnapshot,
    ) -> Result<Self, EvaluationError> {
        if maximum_output_tokens == 0
            || temperature_millionths.is_some_and(|value| value > 2_000_000)
            || top_p_millionths.is_some_and(|value| value > 1_000_000)
            || matches!(seed_delivery, SeedDeliveryPolicy::Required)
                && !provider.supports_sampling_controls()
        {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "model controls are invalid or require unsupported sampling controls",
            ));
        }
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(MODEL_DOMAIN).map_err(codec)?;
        writer.write_u64(maximum_output_tokens).map_err(codec)?;
        write_option_u32(&mut writer, temperature_millionths)?;
        write_option_u32(&mut writer, top_p_millionths)?;
        writer.write_fixed(request_template_digest.as_bytes()).map_err(codec)?;
        writer.write_u8(seed_delivery.tag()).map_err(codec)?;
        writer.write_fixed(provider.digest().as_bytes()).map_err(codec)?;
        let digest = peritus_codec::sha256(&writer.into_bytes());
        Ok(Self {
            maximum_output_tokens,
            temperature_millionths,
            top_p_millionths,
            request_template_digest,
            seed_delivery,
            digest,
        })
    }
    /// Maximum output tokens.
    #[must_use]
    pub const fn maximum_output_tokens(self) -> u64 {
        self.maximum_output_tokens
    }
    /// Temperature in millionths.
    #[must_use]
    pub const fn temperature_millionths(self) -> Option<u32> {
        self.temperature_millionths
    }
    /// Top-p in millionths.
    #[must_use]
    pub const fn top_p_millionths(self) -> Option<u32> {
        self.top_p_millionths
    }
    /// Request-template digest.
    #[must_use]
    pub const fn request_template_digest(self) -> Sha256Digest {
        self.request_template_digest
    }
    /// Seed delivery policy.
    #[must_use]
    pub const fn seed_delivery(self) -> SeedDeliveryPolicy {
        self.seed_delivery
    }
    /// Complete controls digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

fn write_option_u32(
    writer: &mut CanonicalWriter,
    value: Option<u32>,
) -> Result<(), EvaluationError> {
    writer.write_option_tag(value.is_some()).map_err(codec)?;
    if let Some(value) = value {
        writer.write_u32(value).map_err(codec)?;
    }
    Ok(())
}

const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::FreezeProfile,
        EvaluationRecovery::ReduceScope,
        "provider/model snapshot exceeds production codec limits",
    )
}

const fn dialect_tag(value: WireDialect) -> u8 {
    match value {
        WireDialect::OpenAiResponses => 1,
        WireDialect::AnthropicMessages => 2,
        WireDialect::GeminiInteractionsV1 => 3,
        WireDialect::GeminiGenerateContentV1 => 4,
        WireDialect::CompatibleResponses => 5,
        WireDialect::CompatibleChatCompletions => 6,
        WireDialect::OpenAiCodexRuntime => 7,
        WireDialect::AnthropicClaudeRuntime => 8,
    }
}
const fn capability_state_tag(value: CapabilityState) -> u8 {
    match value {
        CapabilityState::Supported => 1,
        CapabilityState::Unsupported => 2,
        CapabilityState::Unknown => 3,
    }
}
const fn provenance_tag(value: CapabilityProvenance) -> u8 {
    match value {
        CapabilityProvenance::Profiled => 1,
        CapabilityProvenance::Discovered => 2,
        CapabilityProvenance::Probed => 3,
    }
}
const fn output_tag(value: OutputLimitEnforcement) -> u8 {
    match value {
        OutputLimitEnforcement::ProviderEnforced => 1,
        OutputLimitEnforcement::Advisory => 2,
    }
}
const fn state_tag(value: StateMode) -> u8 {
    match value {
        StateMode::StatelessReplay => 1,
        StateMode::ProviderStored => 2,
        StateMode::BackgroundResumable => 3,
    }
}
const fn resume_tag(value: ResumeKind) -> u8 {
    match value {
        ResumeKind::Unsupported => 1,
        ResumeKind::SemanticContinuation => 2,
        ResumeKind::ExactCursor => 3,
    }
}
const fn cancellation_tag(value: CancellationKind) -> u8 {
    match value {
        CancellationKind::Unsupported => 1,
        CancellationKind::BestEffortLocalAbort => 2,
        CancellationKind::Confirmed => 3,
    }
}
